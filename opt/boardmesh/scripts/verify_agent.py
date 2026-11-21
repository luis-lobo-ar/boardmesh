#!/usr/bin/env python3
"""Exercise real standalone Boardmesh MCP + CLI without a Mesh service."""
import argparse
import hashlib
import json
import os
import queue
import select
import socket
import struct
import subprocess
import threading
import tempfile
import time
import traceback
from pathlib import Path


def check(condition, name, checks):
    if not condition:
        raise AssertionError(name)
    checks.append(name)


class MCP:
    def __init__(self, binary, session_root, port, evidence):
        self.counter = 0
        self.queue = queue.Queue()
        self.transcript = (evidence / 'mcp-transcript.jsonl').open('a')
        self.log = (evidence / 'mcp-stderr.log').open('a')
        self.process = subprocess.Popen([
            str(binary), '--session', 'agent-diagram', '--sessions-dir', str(session_root),
            '--runtime-addr', f'127.0.0.1:{port}',
        ], cwd=evidence, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log, text=True)
        threading.Thread(target=self._read, daemon=True).start()
        self.rpc('initialize', {'protocolVersion': '2024-11-05', 'capabilities': {}, 'clientInfo': {'name': 'boardmesh-test-agent', 'version': '1'}})
        self._write({'jsonrpc': '2.0', 'method': 'notifications/initialized'})

    def _read(self):
        for line in self.process.stdout:
            self.queue.put(json.loads(line))

    def _write(self, message):
        self.transcript.write(json.dumps({'send': message}) + '\n')
        self.transcript.flush()
        self.process.stdin.write(json.dumps(message) + '\n')
        self.process.stdin.flush()

    def rpc(self, method, params):
        self.counter += 1
        self._write({'jsonrpc': '2.0', 'id': self.counter, 'method': method, 'params': params})
        while True:
            response = self.queue.get(timeout=15)
            self.transcript.write(json.dumps({'receive': response}) + '\n')
            self.transcript.flush()
            if response.get('id') == self.counter:
                return response

    def raw_tool(self, name, arguments):
        return self.rpc('tools/call', {'name': name, 'arguments': arguments})

    def tool(self, name, arguments=None):
        response = self.raw_tool(name, arguments or {})
        if 'error' in response or response['result'].get('isError'):
            raise AssertionError(f'{name}: {response}')
        result = response['result']
        return result.get('structuredContent') or json.loads(result['content'][0]['text'])

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        self.log.close()
        self.transcript.close()


def wait_until(predicate, description, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.1)
    raise AssertionError(description)


def verify_gui(binary, server, session_root, runtime, evidence, checks):
    import smoke_gui
    from Xlib import X, display, protocol

    gui = None
    log = None
    xvfb = subprocess.Popen(['Xvfb', '-displayfd', '1', '-screen', '0', '1600x1200x24'],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        if not select.select([xvfb.stdout], [], [], 5)[0]:
            raise AssertionError('Xvfb did not allocate a display')
        display_name = ':' + xvfb.stdout.readline().strip()
        smoke_gui.DISPLAY = display_name
        env = dict(os.environ, DISPLAY=display_name)
        log_path = evidence / 'gui.log'

        def launch():
            nonlocal gui, log
            log = log_path.open('a')
            gui = subprocess.Popen([str(binary), '--session', 'agent-diagram',
                                    '--sessions-dir', str(session_root), '--runtime-url', runtime,
                                    '--tool', 'line'], cwd=evidence, env=env, stdout=log, stderr=log)
            return smoke_gui.wait_for_window(env, 'Boardmesh')

        def close(window):
            connection = display.Display(display_name)
            target = connection.create_resource_object('window', int(window))
            target.send_event(protocol.event.ClientMessage(
                window=target, client_type=connection.intern_atom('WM_PROTOCOLS'),
                data=(32, [connection.intern_atom('WM_DELETE_WINDOW'), X.CurrentTime, 0, 0, 0])))
            connection.flush()
            connection.close()
            gui.wait(timeout=10)
            log.close()

        window = launch()
        wait_until(lambda: 'applied runtime scene update' in log_path.read_text(),
                   'desktop did not attach to the runtime')
        old_ids = {obj['id'] for obj in server.tool('get_scene')['objects']}
        live = server.tool('add_text', {'position': [100, 440], 'text': 'Live agent update'})
        count = len(old_ids) + 1
        wait_until(lambda: f'object_count={count}' in log_path.read_text()
                   and 'kind=ExternalRefresh' in log_path.read_text(),
                   'desktop did not receive the live agent edit')
        check(gui.poll() is None, 'desktop receives a live agent update', checks)
        time.sleep(0.3)
        subprocess.run(['import', '-display', display_name, '-window', 'root',
                        str(evidence / 'gui-agent-update.png')], check=True)
        smoke_gui.xdotool_drag_window(env, window, 520, 560, 760, 620)
        wait_until(lambda: len(server.tool('get_scene')['objects']) == count + 1,
                   'human drawing did not reach the agent')
        human_scene = server.tool('get_scene')
        expected_ids = old_ids | {live['object_id']}
        check(expected_ids <= {obj['id'] for obj in human_scene['objects']},
              'human GUI drawing reaches MCP and preserves every agent object', checks)
        smoke_gui.focus_window(env, window)
        smoke_gui.xtest_key(display_name, 'z', modifiers=('Control_L',))
        wait_until(lambda: len(server.tool('get_scene')['objects']) == count,
                   'GUI undo did not remove the human stroke')
        check(True, 'GUI undo propagates to the agent', checks)
        # Let queued notifications arrive while redo must remain available.
        time.sleep(0.5)
        smoke_gui.xtest_key(display_name, 'y', modifiers=('Control_L',))
        wait_until(lambda: len(server.tool('get_scene')['objects']) == count + 1,
                   'GUI redo did not restore the human stroke')
        check(True, 'GUI redo propagates to the agent', checks)
        subprocess.run(['import', '-display', display_name, '-window', 'root',
                        str(evidence / 'gui-human-and-agent.png')], check=True)
        saved = server.tool('get_scene')
        close(window)
        reopen_offset = log_path.stat().st_size
        window = launch()
        reopened_scene = session_root / 'agent-diagram.json'
        loaded = f'window initialized session=Some("agent-diagram") scene={reopened_scene} object_count={len(saved["objects"])}'
        wait_until(lambda: loaded in log_path.read_text()[reopen_offset:],
                   'reopened desktop did not load the saved board')
        check(server.tool('get_scene') == saved and gui.poll() is None,
              'desktop reopens the same board without losing human or agent content', checks)
        server.tool('add_text', {'position': [100, 480], 'text': 'Reopened board confirmed'})
        updated_count = len(saved['objects']) + 1
        wait_until(lambda: f'kind=ExternalRefresh session=Some("agent-diagram") scene={reopened_scene} object_count={updated_count}'
                   in log_path.read_text()[reopen_offset:],
                   'reopened desktop did not receive a fresh agent update')
        check(True, 'reopened desktop receives a fresh agent update on the same board', checks)
        time.sleep(0.3)
        subprocess.run(['import', '-display', display_name, '-window', 'root',
                        str(evidence / 'gui-reopened.png')], check=True)
        close(window)
        server.tool('render_scene_png', {'path': str(evidence / 'human-and-agent.png')})
        check(True, 'combined human and agent board exports to PNG', checks)
    finally:
        if gui and gui.poll() is None:
            gui.terminate()
            try:
                gui.wait(timeout=5)
            except subprocess.TimeoutExpired:
                gui.kill()
                gui.wait()
        if log and not log.closed:
            log.close()
        xvfb.terminate()
        xvfb.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bin-dir', type=Path, default=Path(__file__).resolve().parents[3] / 'target/debug')
    parser.add_argument('--gui-binary', type=Path, help='Also verify live desktop edits, undo/redo and reopening under Xvfb')
    parser.add_argument('--evidence-dir', type=Path, default=None)
    args = parser.parse_args()
    bins = args.bin_dir.resolve()
    evidence = (args.evidence_dir or Path(tempfile.gettempdir()) / ('boardmesh-verify-' + str(time.time_ns()))).resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    session_root = evidence / 'sessions'
    checks = []
    server = None
    result = {'bin_dir': str(bins), 'evidence_dir': str(evidence), 'checks': checks}
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        port = probe.getsockname()[1]
    runtime = f'ws://127.0.0.1:{port}'

    def cli(method, parameters):
        command = [str(bins / 'boardmesh-cli'), method, '--runtime-url', runtime, '--json', json.dumps(parameters), '--compact']
        completed = subprocess.run(command, cwd=evidence, capture_output=True, text=True, timeout=10)
        with (evidence / 'cli-transcript.jsonl').open('a') as transcript:
            transcript.write(json.dumps({'command': command, 'returncode': completed.returncode, 'stdout': completed.stdout, 'stderr': completed.stderr}) + '\n')
        if completed.returncode:
            raise AssertionError(completed.stderr)
        return json.loads(completed.stdout)

    try:
        server = MCP(bins / 'boardmesh-mcp', session_root, port, evidence)
        tools = server.rpc('tools/list', {})['result']['tools']
        (evidence / 'tools.json').write_text(json.dumps(tools, indent=2))
        tool_names = {tool['name'] for tool in tools}
        result['tool_count'] = len(tools)
        result['unsupported_tools'] = sorted({'add_connector', 'add_link'} - tool_names)
        check({'add_rectangle', 'add_text', 'add_line', 'create_canvas', 'assign_object_to_canvas', 'render_scene_png'} <= tool_names, 'MCP advertises required actual tools', checks)
        check(server.tool('get_scene')['objects'] == [], 'new named session starts empty', checks)
        canvas = server.tool('create_canvas', {'name': 'Mesh build and review', 'rect': [40, 40, 860, 460]})['canvas_id']
        ids = []
        for rect, color in [([100, 140, 220, 120], {'r': 219, 'g': 234, 'b': 254}), ([540, 140, 220, 120], {'r': 220, 'g': 252, 'b': 231})]:
            ids.append(server.tool('add_rectangle', {'rect': rect, 'fill': color})['object_id'])
        for position, text in [([135, 185], 'Build'), ([575, 185], 'Review'), ([100, 340], 'Agent + human on one board')]:
            ids.append(server.tool('add_text', {'position': position, 'text': text})['object_id'])
        ids.append(server.tool('add_line', {'start': [320, 200], 'end': [540, 200]})['object_id'])
        for object_id in ids:
            server.tool('assign_object_to_canvas', {'object_id': object_id, 'canvas_id': canvas})
        check(len(server.tool('get_scene')['objects']) == 6, 'agent drew two nodes three labels and their connecting line', checks)
        universe = server.tool('get_universe')['universe']
        check(universe['active_canvas_id'] == canvas and len(universe['memberships']) == 6, 'canvas and six memberships persist', checks)
        server.tool('select_objects', {'object_ids': ids[:2]})
        selection = server.tool('get_selection_context')
        (evidence / 'selection-context.json').write_text(json.dumps(selection, indent=2))
        check(set(server.tool('get_scene')['selected_object_ids']) == set(ids[:2]), 'MCP selects both diagram nodes', checks)
        check(selection['context']['kind'] == 'multi_object' and set(selection['context']['selected_object_ids']) == set(ids[:2]), 'semantic selection identifies both selected nodes', checks)
        check(all(x['canvas_id'] == canvas and x['semantic_kind'] == 'rectangle' for x in selection['context']['selected_objects']), 'semantic selection reports correct canvas and object kinds', checks)
        before_invalid = server.tool('get_scene')
        invalid = server.raw_tool('add_rectangle', {'rect': 'invalid rectangle'})
        check('error' in invalid or invalid.get('result', {}).get('isError'), 'malformed rectangle is rejected', checks)
        check(server.tool('get_scene') == before_invalid, 'malformed MCP call does not mutate board', checks)
        missing = server.raw_tool('delete_object', {'object_id': 'missing-object'})
        result['missing_object_response'] = missing
        check('error' in missing or missing.get('result', {}).get('isError'), 'missing object deletion is rejected', checks)
        server.tool('set_object_visibility', {'object_id': ids[0], 'visible': False})
        check(not server.tool('get_scene')['objects'][0]['visible'], 'agent can hide an object', checks)
        server.tool('set_object_visibility', {'object_id': ids[0], 'visible': True})
        server.tool('set_object_locked', {'object_id': ids[0], 'locked': True})
        check(server.tool('get_scene')['objects'][0]['locked'], 'agent can lock an object', checks)
        server.tool('set_object_locked', {'object_id': ids[0], 'locked': False})
        deadline = time.monotonic() + 5
        while True:
            try:
                observed = cli('get_scene', {'session_id': 'agent-diagram'})
                break
            except AssertionError:
                if time.monotonic() >= deadline:
                    raise
                time.sleep(0.1)
        check(len(observed['objects']) == 6, 'standalone CLI sees MCP board edits', checks)
        copied = cli('add_text', {'session_id': 'agent-diagram', 'position': [100, 380], 'text': 'CLI confirmed'})
        check(copied['ok'] and len(server.tool('get_scene')['objects']) == 7, 'MCP observes a CLI edit', checks)
        server.tool('create_session', {'session_id': 'other-board', 'make_active': True})
        check(server.tool('get_scene')['objects'] == [], 'second session is isolated', checks)
        server.tool('add_text', {'position': [80, 80], 'text': 'Other board only'})
        check(len(cli('get_scene', {'session_id': 'agent-diagram'})['objects']) == 7, 'CLI explicit session scope preserves original board', checks)
        server.tool('switch_session', {'session_id': 'agent-diagram'})
        check(len(server.tool('get_scene')['objects']) == 7, 'MCP returns to original session intact', checks)
        exported = server.tool('export_scene_json')
        (evidence / 'scene-export-response.json').write_text(json.dumps(exported, indent=2))
        scene_json = exported.get('scene_json') or exported.get('json')
        check(scene_json is not None and len(json.loads(scene_json)['objects']) == 7, 'JSON export preserves all seven objects', checks)
        (evidence / 'diagram.json').write_text(scene_json)
        png = evidence / 'diagram.png'
        server.tool('render_scene_png', {'path': str(png), 'padding': 24})
        data = png.read_bytes()
        check(data[:8] == b'\x89PNG\r\n\x1a\n', 'PNG export has a real PNG signature', checks)
        width, height = struct.unpack('>II', data[16:24])
        check(width > 600 and height > 200, 'PNG dimensions include the diagram', checks)
        result['png_dimensions'] = [width, height]
        selected_png = evidence / 'selected.png'
        server.tool('render_selected_png', {'path': str(selected_png)})
        check(selected_png.read_bytes()[:8] == b'\x89PNG\r\n\x1a\n', 'selected-object PNG export works', checks)
        if args.gui_binary:
            verify_gui(args.gui_binary.resolve(), server, session_root, runtime, evidence, checks)
        saved = server.tool('get_universe')['universe']
        server.close()
        server = MCP(bins / 'boardmesh-mcp', session_root, port, evidence)
        check(server.tool('get_universe')['universe'] == saved, 'full universe survives MCP process restart', checks)
        check(len(cli('get_scene', {'session_id': 'other-board'})['objects']) == 1, 'other session survives restart independently', checks)
        result['ok'] = True
        result['session_path'] = str(session_root / 'agent-diagram.json')
        result['png_sha256'] = hashlib.sha256(data).hexdigest()
    except Exception as error:
        result['ok'] = False
        result['error'] = str(error)
        result['traceback'] = traceback.format_exc()
    finally:
        if server:
            server.close()
        (evidence / 'result.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))
    return 0 if result.get('ok') else 1


if __name__ == '__main__':
    raise SystemExit(main())
