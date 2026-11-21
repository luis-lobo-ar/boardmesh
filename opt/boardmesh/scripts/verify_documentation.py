#!/usr/bin/env python3
"""Check documentation links and run the documented example against actual MCP tools."""
import argparse
from html.parser import HTMLParser
import json
from pathlib import Path
import socket
import string
import tempfile
from urllib.parse import unquote, urlsplit

from verify_agent import MCP


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin-dir", type=Path, required=True)
    args = parser.parse_args()
    product = Path(__file__).resolve().parents[1]
    docs = product / "docs"
    missing = []
    class Links(HTMLParser):
        def handle_starttag(self, tag, attrs):
            for key, value in attrs:
                if key not in {"href", "src"} or not value:
                    continue
                url = urlsplit(value)
                if url.scheme or url.netloc or not url.path:
                    continue
                if not (page.parent / unquote(url.path)).exists():
                    missing.append(f"{page.relative_to(product)} -> {value}")
    for page in docs.rglob("*.html"):
        if "projects" not in page.relative_to(docs).parts:
            Links().feed(page.read_text())
    assert not missing, missing
    catalog = json.loads((docs / "tool-reference.json").read_text())
    evidence = Path(tempfile.mkdtemp(prefix="boardmesh-docs-example-"))
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    server = MCP(args.bin_dir.resolve() / "boardmesh-mcp", evidence / "sessions", port, evidence)
    variables = {"output_dir": str(evidence)}
    def resolve(value):
        if isinstance(value, str):
            return string.Template(value).substitute(variables)
        if isinstance(value, list):
            return [resolve(item) for item in value]
        if isinstance(value, dict):
            return {key: resolve(item) for key, item in value.items()}
        return value
    try:
        actual = sorted(server.rpc("tools/list", {})["result"]["tools"], key=lambda tool: tool["name"])
        assert actual == catalog["tools"], "Generated MCP reference drifted from the binary"
        steps = json.loads((docs / "examples/first-board.json").read_text())
        for step in steps:
            result = server.tool(step["tool"], resolve(step["arguments"]))
            if "save_id" in step:
                variables[step["save_id"]] = result.get("object_id") or result["canvas_id"]
        scene = server.tool("get_scene")
        assert len(scene["objects"]) == 5
        assert scene["selected_object_ids"] == [variables["build"]]
        assert (evidence / "first-board.png").read_bytes().startswith(b"\x89PNG\r\n\x1a\n")
        context = server.tool("get_selection_context")["context"]
        assert context["selected_objects"][0]["canvas_id"] == variables["canvas"]
        (evidence / "result.json").write_text(json.dumps({"ok": True, "steps": len(steps), "scene": scene}, indent=2))
    finally:
        server.close()
    print(f"Documentation links, complete tool catalog, and {len(steps)} example calls passed. Evidence: {evidence}")


if __name__ == "__main__":
    main()
