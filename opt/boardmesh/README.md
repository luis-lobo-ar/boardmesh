# Boardmesh

Boardmesh is a local desktop board that people and agents can edit together. Draw a sketch, select an object, and ask an agent to inspect or change it. The agent reads structured objects and selection context through MCP. The desktop shows the result.

Use Boardmesh for architecture reviews, code explanations, incident timelines, planning, and visual handoffs. Boards remain local JSON files. Boardmesh runs independently of other Mesh services.

**[Read the documentation](https://luis-lobo-ar.github.io/boardmesh/)**

- [Overview and benefits](docs/overview.html)
- [Create your first shared board](docs/getting-started.html)
- [Work with people and agents](docs/people-and-agents.html)
- [Use cases and prompts](docs/use-cases.html)
- [Desktop reference](docs/desktop.html)
- [Agent guide](docs/agent-guide.html) and [complete tool reference](docs/tool-reference.html)
- [Files and runtime](docs/data-and-runtime.html)
- [Compatible connector routing](docs/routing.html)
- [Troubleshooting](docs/troubleshooting.html)
- [Development and verification](docs/development.html)

## Install

Download a Linux x86_64 snapshot and its checksum from [Releases](https://github.com/luis-lobo-ar/boardmesh/releases). Follow the [installation guide](docs/install-and-verification.html) to verify and extract it, install native desktop libraries, and choose an explicit data directory.

The archive contains four executables:

| Executable | Purpose |
| --- | --- |
| `boardmesh` | Desktop editor |
| `boardmesh-mcp` | MCP stdio server and local runtime authority |
| `boardmesh-cli` | Command-line client for the runtime |
| `boardmesh-layout-lab` | Desktop layout development tool |

Connect one MCP client to `boardmesh-mcp`. Open the desktop against the same sessions directory, session ID, and runtime URL. The [first-board tutorial](docs/getting-started.html) gives exact commands and tested tool calls.

## Build from source

Use Rust 1.95 or newer. Run from the workspace root:

```sh
cargo build --locked -p boardmesh --bin boardmesh-cli --bin boardmesh-mcp
cargo test --locked -p boardmesh
```

For the desktop, install the [native development dependencies](docs/install-and-verification.html), then build:

```sh
RUSTFLAGS='-C link-arg=-lSDL' cargo build --locked -p boardmesh --features gui
```

Generate and validate API documentation against actual binaries:

```sh
python3 opt/boardmesh/scripts/generate_tool_reference.py --bin-dir target/debug
python3 opt/boardmesh/scripts/verify_documentation.py --bin-dir target/debug
python3 opt/boardmesh/scripts/verify_agent.py --bin-dir target/debug \
  --gui-binary target/debug/boardmesh
```

The desktop verifier requires Xvfb, xdotool, ImageMagick, and Python Xlib. It tests real MCP and CLI calls, GUI drawing, live updates, undo/redo, persistence, and PNG export in isolated temporary sessions.

## Compatibility and current limits

Existing boards retain their JSON format, IDs, and endpoint bindings. A guarded `ortho-router` adapter improves blocked orthogonal connectors. Clear routes, straight and spline connectors, ordinary lines, and unsupported cases retain their existing geometry. [Routing details and comparison evidence](docs/routing.html) describe the bounds and fallbacks.

The release advertises 40 MCP tools and 45 runtime methods. Some richer scene-model objects have no dedicated MCP creation tool. Undo belongs to the local desktop window. There is no browser editor, hosted collaboration service, or automatic WorkMesh task synchronization. Optional toolbar icons use fallback shapes when the original source assets are unavailable.

## Source and distribution

Authoritative development lives in the Mesh monorepo under `opt/boardmesh`. The [public standalone repository](https://github.com/luis-lobo-ar/boardmesh) receives generated snapshots of Boardmesh and its required shared libraries. It builds without private Mesh access or another Mesh product. The internal `mesh-id` helper remains unpublished and is included as source.

The standalone GitHub workflow tests the product and shared libraries, builds four Linux binaries, and tests an extracted release before publishing an immutable snapshot. A separate workflow publishes these HTML manuals to GitHub Pages. [Development](docs/development.html) explains source ownership, provenance, and contribution boundaries.

Mesh code is MIT-licensed. Preserve the third-party notices distributed with source and binaries.
