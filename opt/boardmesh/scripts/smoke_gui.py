#!/usr/bin/env python3

import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import argparse
from pathlib import Path
from Xlib import X, XK, display
from Xlib.ext import xtest


ROOT = Path(__file__).resolve().parents[1]
WORKSPACE_ROOT = ROOT.parents[1]
APP = WORKSPACE_ROOT / "target" / "debug" / "boardmesh"
DEFAULT_SCENE = ROOT / "boardmesh-scene.json"
DISPLAY = ":99"


def run(cmd, env=None, check=True, capture_output=True):
    return subprocess.run(
        cmd,
        env=env,
        check=check,
        text=True,
        capture_output=capture_output,
    )


def wait_for_window(env, title, timeout_seconds=15):
    deadline = time.time() + timeout_seconds
    while time.time() < deadline:
        result = run(
            ["xdotool", "search", "--onlyvisible", "--name", title],
            env=env,
            check=False,
        )
        window_ids = [line.strip() for line in result.stdout.splitlines() if line.strip()]
        if window_ids:
            return window_ids[0]
        time.sleep(0.2)
    raise RuntimeError(f"timed out waiting for window {title!r}")


def get_geometry(env, window_id):
    result = run(["xdotool", "getwindowgeometry", "--shell", window_id], env=env)
    geometry = {}
    for line in result.stdout.splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            geometry[key] = int(value)
    return geometry


def scene_object_count(scene_path):
    with scene_path.open() as handle:
        return len(json.load(handle)["objects"])


def load_scene(scene_path):
    with scene_path.open() as handle:
        return json.load(handle)


def save_scene(scene_path, scene):
    with scene_path.open("w") as handle:
        json.dump(scene, handle, indent=2)
        handle.write("\n")


def empty_scene():
    return {
        "version": 1,
        "background": {"r": 250, "g": 250, "b": 249, "a": 255},
        "selected_object_ids": [],
        "objects": [],
    }


def rectangle_fill(scene_path):
    scene = {
        "version": 1,
        "background": {"r": 250, "g": 250, "b": 249, "a": 255},
        "objects": [
            {
                "id": "rect-under-test",
                "kind": {
                    "type": "rectangle",
                    "rect": {"x": 80, "y": 80, "width": 420, "height": 260},
                },
                "stroke": {
                    "color": {"r": 15, "g": 23, "b": 42, "a": 255},
                    "width": 2,
                },
                "fill": None,
            }
        ],
    }
    save_scene(scene_path, scene)


def rectangle_scene(scene_path):
    scene = {
        "version": 1,
        "background": {"r": 250, "g": 250, "b": 249, "a": 255},
        "selected_object_ids": [],
        "objects": [
            {
                "id": "rect-under-test",
                "kind": {
                    "type": "rectangle",
                    "rect": {"x": 80, "y": 80, "width": 420, "height": 260},
                },
                "stroke": {
                    "color": {"r": 15, "g": 23, "b": 42, "a": 255},
                    "width": 2,
                },
                "fill": None,
            }
        ],
    }
    save_scene(scene_path, scene)


def rectangle_selected_scene(scene_path):
    scene = {
        "version": 1,
        "background": {"r": 250, "g": 250, "b": 249, "a": 255},
        "selected_object_ids": ["rect-under-test"],
        "objects": [
            {
                "id": "rect-under-test",
                "kind": {
                    "type": "rectangle",
                    "rect": {"x": 80, "y": 80, "width": 420, "height": 260},
                },
                "stroke": {
                    "color": {"r": 15, "g": 23, "b": 42, "a": 255},
                    "width": 2,
                },
                "fill": None,
            }
        ],
    }
    save_scene(scene_path, scene)


def rectangle_is_filled(scene_path):
    scene = load_scene(scene_path)
    objects = scene.get("objects", [])
    if not objects:
        return False
    return objects[0].get("fill") is not None


def selected_object_ids(scene_path):
    scene = load_scene(scene_path)
    return scene.get("selected_object_ids", [])


def xtest_ctrl_click(display_name, absolute_x, absolute_y):
    x_display = display.Display(display_name)
    root = x_display.screen().root
    control_keycode = x_display.keysym_to_keycode(XK.string_to_keysym("Control_L"))

    xtest.fake_input(x_display, X.MotionNotify, x=absolute_x, y=absolute_y, root=root)
    xtest.fake_input(x_display, X.KeyPress, detail=control_keycode)
    xtest.fake_input(x_display, X.ButtonPress, detail=1)
    xtest.fake_input(x_display, X.ButtonRelease, detail=1)
    xtest.fake_input(x_display, X.KeyRelease, detail=control_keycode)
    x_display.sync()
    x_display.close()


def xtest_click(display_name, absolute_x, absolute_y, button=1):
    x_display = display.Display(display_name)
    root = x_display.screen().root
    xtest.fake_input(x_display, X.MotionNotify, x=absolute_x, y=absolute_y, root=root)
    xtest.fake_input(x_display, X.ButtonPress, detail=button)
    xtest.fake_input(x_display, X.ButtonRelease, detail=button)
    x_display.sync()
    x_display.close()


def focus_window(env, window_id):
    run(["xdotool", "windowfocus", window_id], env=env, check=False)


def xdotool_click_window(env, window_id, relative_x, relative_y, button=1):
    focus_window(env, window_id)
    geometry = get_geometry(env, window_id)
    xtest_click(
        DISPLAY,
        geometry["X"] + relative_x,
        geometry["Y"] + relative_y,
        button=button,
    )


def xdotool_drag_window(env, window_id, start_x, start_y, end_x, end_y, button=1):
    focus_window(env, window_id)
    run(
        ["xdotool", "mousemove", "--window", window_id, str(start_x), str(start_y)],
        env=env,
        check=False,
    )
    time.sleep(0.05)
    run(["xdotool", "mousedown", str(button)], env=env, check=False)
    for step in range(1, 7):
        current_x = round(start_x + ((end_x - start_x) * step / 6))
        current_y = round(start_y + ((end_y - start_y) * step / 6))
        run(
            ["xdotool", "mousemove", "--window", window_id, str(current_x), str(current_y)],
            env=env,
            check=False,
        )
        time.sleep(0.05)
    run(["xdotool", "mouseup", str(button)], env=env, check=False)


def xtest_focus_window(display_name, window_id):
    x_display = display.Display(display_name)
    window = x_display.create_resource_object("window", int(window_id))
    window.set_input_focus(X.RevertToParent, X.CurrentTime)
    x_display.sync()
    x_display.close()


def xtest_key(display_name, key_name, modifiers=()):
    x_display = display.Display(display_name)
    modifier_keycodes = [x_display.keysym_to_keycode(XK.string_to_keysym(name)) for name in modifiers]
    keycode = x_display.keysym_to_keycode(XK.string_to_keysym(key_name))

    for modifier_keycode in modifier_keycodes:
        xtest.fake_input(x_display, X.KeyPress, detail=modifier_keycode)
    xtest.fake_input(x_display, X.KeyPress, detail=keycode)
    xtest.fake_input(x_display, X.KeyRelease, detail=keycode)
    for modifier_keycode in reversed(modifier_keycodes):
        xtest.fake_input(x_display, X.KeyRelease, detail=modifier_keycode)

    x_display.sync()
    x_display.close()


def latest_object_is_filled(scene_path):
    scene = load_scene(scene_path)
    objects = scene.get("objects", [])
    if not objects:
        return False
    return objects[-1].get("fill") is not None


def latest_object_type(scene_path):
    scene = load_scene(scene_path)
    objects = scene.get("objects", [])
    if not objects:
        return None
    return objects[-1].get("kind", {}).get("type")


def load_json(path):
    with path.open() as handle:
        return json.load(handle)


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--scenario",
        choices=(
            "draw",
            "triangle",
            "triangle-3-point",
            "polygon",
            "select-tool",
            "select-box",
            "fill",
            "filled-shape",
            "ctrl-select",
            "delete",
            "undo-delete",
            "session",
            "session-picker",
            "switch-session",
        ),
        default="draw",
        help="Headless GUI scenario to run",
    )
    return parser.parse_args()


def prepare_named_session(scene_path):
    save_scene(scene_path, empty_scene())


def main():
    args = parse_args()

    print("building desktop binary...", flush=True)
    run(["cargo", "build", "--bin", "boardmesh", "--features", "gui"], env=os.environ.copy())

    temp_dir = Path(tempfile.mkdtemp(prefix="boardmesh-smoke-"))
    scene_path = temp_dir / "scene.json"
    sessions_dir = temp_dir / "sessions"
    session_id = "smoke-session"
    session_scene_path = sessions_dir / f"{session_id}.json"
    session_manifest_path = sessions_dir / f"{session_id}.manifest.json"
    picker_session_id = "picker-session"
    picker_scene_path = sessions_dir / f"{picker_session_id}.json"
    picker_manifest_path = sessions_dir / f"{picker_session_id}.manifest.json"
    alpha_session_id = "alpha"
    beta_session_id = "beta"
    alpha_scene_path = sessions_dir / f"{alpha_session_id}.json"
    beta_scene_path = sessions_dir / f"{beta_session_id}.json"
    alpha_manifest_path = sessions_dir / f"{alpha_session_id}.manifest.json"
    beta_manifest_path = sessions_dir / f"{beta_session_id}.manifest.json"
    screenshot_path = temp_dir / f"after-{args.scenario}.png"
    save_scene(scene_path, empty_scene())
    if args.scenario == "fill":
        rectangle_fill(scene_path)
    elif args.scenario in {"ctrl-select", "select-tool", "select-box"}:
        rectangle_scene(scene_path)
    elif args.scenario in {"delete", "undo-delete"}:
        rectangle_selected_scene(scene_path)
    elif args.scenario == "session-picker":
        sessions_dir.mkdir(parents=True, exist_ok=True)
        prepare_named_session(picker_scene_path)
    elif args.scenario == "switch-session":
        sessions_dir.mkdir(parents=True, exist_ok=True)
        prepare_named_session(alpha_scene_path)
        prepare_named_session(beta_scene_path)
    initial_mtime_ns = scene_path.stat().st_mtime_ns

    xvfb = subprocess.Popen(
        ["Xvfb", DISPLAY, "-screen", "0", "1600x1200x24"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    env = os.environ.copy()
    env["DISPLAY"] = DISPLAY

    app = None
    try:
        time.sleep(0.5)
        command = [str(APP), "--no-runtime", "--scene", str(scene_path)]
        if args.scenario == "session":
            command = [
                str(APP),
                "--no-runtime",
                "--sessions-dir",
                str(sessions_dir),
                "--session",
                session_id,
            ]
        elif args.scenario == "session-picker":
            command = [str(APP), "--no-runtime", "--sessions-dir", str(sessions_dir)]
        elif args.scenario == "switch-session":
            command = [
                str(APP),
                "--no-runtime",
                "--sessions-dir",
                str(sessions_dir),
                "--session",
                alpha_session_id,
            ]
        elif args.scenario == "fill":
            command.extend(["--tool", "fill"])
        elif args.scenario == "filled-shape":
            command.extend(["--tool", "rectangle", "--filled-shapes"])
        elif args.scenario == "triangle":
            command.extend(["--tool", "triangle"])
        elif args.scenario == "triangle-3-point":
            command.extend(["--tool", "triangle", "--triangle-mode", "3-point"])
        elif args.scenario in {"select-tool", "select-box"}:
            command.extend(["--tool", "select"])
        elif args.scenario == "polygon":
            command.extend(["--tool", "polygon"])
        elif args.scenario == "draw":
            command.extend(["--tool", "line"])
        app = subprocess.Popen(
            command,
            cwd=ROOT,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        if args.scenario == "session-picker":
            dialog_id = wait_for_window(env, "Open Board Session")
            focus_window(env, dialog_id)
            xtest_key(DISPLAY, "Return")
            window_id = wait_for_window(env, "Boardmesh")
        elif args.scenario == "switch-session":
            window_id = wait_for_window(env, "Boardmesh")
            focus_window(env, window_id)
            time.sleep(0.2)
            xtest_key(DISPLAY, "Tab")
            time.sleep(0.1)
            xtest_key(DISPLAY, "Return")
            dialog_id = wait_for_window(env, "Switch Session")
            focus_window(env, dialog_id)
            xtest_key(DISPLAY, "Down")
            time.sleep(0.05)
            xtest_key(DISPLAY, "Return")
        else:
            window_id = wait_for_window(env, "Boardmesh")
            focus_window(env, window_id)
            time.sleep(0.2)

        if args.scenario == "session-picker":
            focus_window(env, window_id)
            time.sleep(0.2)

        if args.scenario == "fill":
            xdotool_click_window(env, window_id, 560, 260)
        elif args.scenario == "ctrl-select":
            geometry = get_geometry(env, window_id)
            xtest_ctrl_click(DISPLAY, geometry["X"] + 560, geometry["Y"] + 260)
        elif args.scenario == "select-tool":
            xdotool_click_window(env, window_id, 560, 260)
        elif args.scenario == "select-box":
            xdotool_drag_window(env, window_id, 500, 180, 860, 520)
        elif args.scenario == "polygon":
            polygon_points = [
                (520, 190),
                (740, 190),
                (780, 330),
                (600, 390),
            ]
            for x, y in polygon_points:
                xdotool_click_window(env, window_id, x, y)
                time.sleep(0.12)
            xdotool_click_window(
                env,
                window_id,
                polygon_points[-1][0],
                polygon_points[-1][1],
                button=3,
            )
        elif args.scenario == "triangle-3-point":
            triangle_points = [
                (620, 180),
                (520, 380),
                (780, 320),
            ]
            for x, y in triangle_points:
                xdotool_click_window(env, window_id, x, y)
                time.sleep(0.15)
            xtest_key(DISPLAY, "Return")
        elif args.scenario == "delete":
            xtest_key(DISPLAY, "Delete")
        elif args.scenario == "undo-delete":
            xtest_key(DISPLAY, "Delete")
            time.sleep(0.1)
            xtest_key(DISPLAY, "z", modifiers=("Control_L",))
        elif args.scenario == "filled-shape":
            xdotool_drag_window(env, window_id, 520, 260, 760, 460)
        else:
            xdotool_drag_window(env, window_id, 520, 260, 760, 460)

        deadline = time.time() + 5
        while time.time() < deadline:
            if app.poll() is not None:
                stdout, stderr = app.communicate(timeout=1)
                raise RuntimeError(
                    f"app exited early with code {app.returncode}\nstdout:\n{stdout}\nstderr:\n{stderr}"
                )

            if args.scenario == "fill":
                if rectangle_is_filled(scene_path):
                    break
            elif args.scenario == "session":
                if session_scene_path.exists() and session_manifest_path.exists():
                    manifest = load_json(session_manifest_path)
                    windows = manifest.get("attached_windows", [])
                    if (
                        manifest.get("session_id") == session_id
                        and manifest.get("scene_path") == str(session_scene_path)
                        and manifest.get("protocol_version") == 1
                        and windows
                    ):
                        break
            elif args.scenario == "session-picker":
                if picker_scene_path.exists() and picker_manifest_path.exists():
                    manifest = load_json(picker_manifest_path)
                    windows = manifest.get("attached_windows", [])
                    if (
                        manifest.get("session_id") == picker_session_id
                        and manifest.get("scene_path") == str(picker_scene_path)
                        and windows
                    ):
                        break
            elif args.scenario == "switch-session":
                if beta_manifest_path.exists():
                    beta_manifest = load_json(beta_manifest_path)
                    beta_windows = beta_manifest.get("attached_windows", [])
                    alpha_windows = []
                    if alpha_manifest_path.exists():
                        alpha_windows = load_json(alpha_manifest_path).get("attached_windows", [])
                    if (
                        beta_manifest.get("session_id") == beta_session_id
                        and beta_manifest.get("scene_path") == str(beta_scene_path)
                        and beta_windows
                        and not alpha_windows
                    ):
                        break
            elif args.scenario == "ctrl-select":
                if "rect-under-test" in selected_object_ids(scene_path):
                    break
            elif args.scenario in {"select-tool", "select-box"}:
                if scene_object_count(scene_path) == 1 and "rect-under-test" in selected_object_ids(scene_path):
                    break
            elif args.scenario == "polygon":
                if latest_object_type(scene_path) == "polygon":
                    break
            elif args.scenario == "triangle-3-point":
                if latest_object_type(scene_path) == "triangle":
                    break
            elif args.scenario == "delete":
                if scene_object_count(scene_path) == 0:
                    break
            elif args.scenario == "undo-delete":
                if scene_object_count(scene_path) == 1 and scene_path.stat().st_mtime_ns > initial_mtime_ns:
                    break
            elif args.scenario == "filled-shape":
                if latest_object_is_filled(scene_path):
                    break
            elif scene_object_count(scene_path) > 0:
                break
            time.sleep(0.2)
        else:
            run(
                ["import", "-display", DISPLAY, "-window", "root", str(screenshot_path)],
                env=env,
                check=False,
            )
            if args.scenario == "fill":
                raise RuntimeError("scene file did not record a fill after automation")
            if args.scenario == "session":
                raise RuntimeError("session launch did not create a scene file and manifest with an attached window heartbeat")
            if args.scenario == "session-picker":
                raise RuntimeError("session picker launch did not attach the window to the chosen session")
            if args.scenario == "switch-session":
                raise RuntimeError("switch session button did not move the window attachment to the selected session")
            if args.scenario == "ctrl-select":
                raise RuntimeError("scene file did not record ctrl-click selection after automation")
            if args.scenario == "select-tool":
                raise RuntimeError("select tool did not record a click selection without drawing")
            if args.scenario == "select-box":
                raise RuntimeError("select tool did not record a drag-box selection without drawing")
            if args.scenario == "polygon":
                raise RuntimeError("scene file did not record a polygon after automation")
            if args.scenario == "triangle-3-point":
                raise RuntimeError("scene file did not record a 3-point triangle after automation")
            if args.scenario == "delete":
                raise RuntimeError("scene file did not delete selected objects after automation")
            if args.scenario == "undo-delete":
                raise RuntimeError("scene file did not restore objects after undo automation")
            if args.scenario == "filled-shape":
                raise RuntimeError("scene file did not record a filled shape after automation")
            raise RuntimeError("scene file did not change after drawing automation")

        run(["import", "-display", DISPLAY, "-window", "root", str(screenshot_path)], env=env)

        print(f"{args.scenario} smoke test passed; screenshot: {screenshot_path}")
        print(f"scene file: {scene_path}")
        return 0
    finally:
        if app is not None and app.poll() is None:
            app.send_signal(signal.SIGTERM)
            try:
                app.wait(timeout=3)
            except subprocess.TimeoutExpired:
                app.kill()
        xvfb.terminate()
        try:
            xvfb.wait(timeout=3)
        except subprocess.TimeoutExpired:
            xvfb.kill()


if __name__ == "__main__":
    sys.exit(main())
