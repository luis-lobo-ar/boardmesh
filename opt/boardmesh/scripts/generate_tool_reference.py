#!/usr/bin/env python3
"""Generate the Boardmesh tool reference from its actual MCP and CLI binaries."""
import argparse
import html
import json
import socket
import subprocess
import tempfile
from pathlib import Path

from verify_agent import MCP


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin-dir", type=Path, required=True)
    parser.add_argument("--docs-dir", type=Path, default=Path(__file__).resolve().parents[1] / "docs")
    args = parser.parse_args()
    bins = args.bin_dir.resolve()
    docs = args.docs_dir.resolve()
    with tempfile.TemporaryDirectory(prefix="boardmesh-reference-") as directory:
        root = Path(directory)
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        server = MCP(bins / "boardmesh-mcp", root / "sessions", port, root)
        try:
            tools = sorted(server.rpc("tools/list", {})["result"]["tools"], key=lambda tool: tool["name"])
            api = server.tool("get_api_docs")
        finally:
            server.close()
    runtime = json.loads(subprocess.check_output([str(bins / "boardmesh-cli"), "list-methods"], text=True))
    metadata = {entry["tool_name"]: entry for entry in api["tools"]}
    assert {tool["name"] for tool in tools} == set(metadata), "Tool metadata must cover the actual catalog"
    catalog = {"format_version": 1, "tools": tools, "tool_help": metadata,
               "runtime_methods": runtime, "conventions": api["conventions"]}
    docs.mkdir(parents=True, exist_ok=True)
    (docs / "tool-reference.json").write_text(json.dumps(catalog, indent=2, sort_keys=True) + "\n")
    escape = html.escape
    sections = []
    for tool in tools:
        name = tool["name"]
        entry = metadata[name]
        schema = tool["inputSchema"]
        required = set(schema.get("required", []))
        fields = []
        for field in entry.get("request_fields", []):
            key = field["name"]
            fields.append(f'<tr><td><code>{escape(key)}</code></td><td>{"Required" if key in required else "Optional"}</td><td>{escape(field["description"])}</td></tr>')
        field_table = ('<table><thead><tr><th>Field</th><th>Presence</th><th>Meaning</th></tr></thead><tbody>' + ''.join(fields) + '</tbody></table>') if fields else '<p>No request fields. Pass an empty object.</p>'
        examples = ''.join('<pre><code>' + escape(example) + '</code></pre>' for example in entry.get("example_args", []))
        returns = ''.join('<li>' + escape(value) + '</li>' for value in entry.get("returns", []))
        output_schema = ('<details><summary>Complete output JSON Schema</summary><pre><code>' + escape(json.dumps(tool['outputSchema'], indent=2, sort_keys=True)) + '</code></pre></details>') if tool.get('outputSchema') else ''
        sections.append(f'<section id="{escape(name)}"><h2><code>{escape(name)}</code></h2><p>{escape(tool.get("description", ""))}</p>{field_table}<h3>Examples</h3>{examples}<h3>Response fields</h3><ul>{returns}</ul><details><summary>Complete input JSON Schema</summary><pre><code>{escape(json.dumps(schema, indent=2, sort_keys=True))}</code></pre></details>{output_schema}</section>')
    links = ''.join(f'<li><a href="#{escape(tool["name"])}"><code>{escape(tool["name"])}</code></a></li>' for tool in tools)
    runtime_only = sorted(set(runtime) - {tool["name"] for tool in tools})
    conventions = ''.join('<li>' + escape(value) + '</li>' for value in api['conventions'])
    content = f'''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Boardmesh tool reference</title><link rel="stylesheet" href="assets/docs.css"></head><body>
<header><a class="brand" href="README.html">Boardmesh</a><span>Tool reference</span></header>
<div class="layout"><nav aria-label="Documentation"><a href="README.html">Documentation home</a><a href="agent-guide.html">Agent guide</a><a href="people-and-agents.html">People and agents</a><a href="data-and-runtime.html">Files and runtime</a><a href="tool-reference.json">Machine-readable reference</a></nav>
<main><p class="eyebrow">Reference · generated from actual binaries</p><h1>MCP tools and runtime methods</h1>
<p>The MCP server advertises {len(tools)} tools. The CLI exposes {len(runtime)} runtime methods. Inspect <code>tools/list</code>, <code>get_tool_help</code>, or <code>get_api_docs</code> when working with another build.</p>
<p>Each example below is an MCP tool argument object. For one-shot CLI calls, add <code>session_id</code> to the argument object when you need explicit session scope. MCP tools instead use the server's active session.</p>
<ul>{conventions}</ul><p>Runtime-only methods: {', '.join('<code>' + escape(name) + '</code>' for name in runtime_only)}. See <a href="data-and-runtime.html">files and runtime reference</a> for their request shapes.</p>
<h2>Tool index</h2><ul class="columns">{links}</ul>{''.join(sections)}
<h2 id="regenerate">Regenerate this reference</h2><pre><code>python3 opt/boardmesh/scripts/generate_tool_reference.py --bin-dir target/debug</code></pre>
<p>The generator starts an isolated temporary MCP server and reads the actual catalog and parameter help. It does not use your normal boards.</p></main></div></body></html>'''
    (docs / "tool-reference.html").write_text(content + "\n")
    print(f"Generated reference: {len(tools)} MCP tools and {len(runtime)} runtime methods")


if __name__ == "__main__":
    main()
