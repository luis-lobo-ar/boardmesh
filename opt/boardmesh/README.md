# Boardmesh

`Boardmesh` ships a local-first board built for human + agent collaboration.

Standalone distribution: [luis-lobo-ar/boardmesh](https://github.com/luis-lobo-ar/boardmesh). Clone that repository to build only Boardmesh and its required shared crates, or download Linux snapshot binaries from its Releases page. This directory remains the authoritative source; the standalone repository receives generated snapshots. The external export supplies its own root Cargo workspace, without creating another workspace inside Mesh.

It currently ships four executables:

- `boardmesh`: a desktop GUI for drawing and inspecting boards
- `boardmesh-layout-lab`: a stripped-down GUI harness for testing pane and toolbar layout behavior
- `boardmesh-mcp`: an MCP stdio server so tools like Codex can create and edit the same board programmatically
- `boardmesh-cli`: a terminal client for the local runtime command surface

The project is intentionally simple: board documents are JSON, sessions are named JSON files plus manifests, and the GUI and MCP server both operate on the same storage model. Raw scene-only JSON still loads and migrates into the newer universe/canvas document shape at the storage layer.

For ecosystem-wide deployment and filesystem guidance, see [../../share/doc/architecture/agent-operating-environment.html](../../share/doc/architecture/agent-operating-environment.html). Boardmesh remains local-first; only its non-GUI surfaces are reasonable container candidates, and those are still single-authority over the files they own.
For shared room, work, deployment, and agent references around Boardmesh sessions, canvases, and objects, see [../../share/doc/architecture/cross-product-contracts.html](../../share/doc/architecture/cross-product-contracts.html).

## Why This Exists

Most whiteboards are optimized for humans only. Most diagram generators are optimized for one-shot output. This project sits in the middle:

- you can sketch locally
- an MCP client can add or edit shapes
- the GUI live-reloads changes from the MCP side
- sessions make it possible to keep multiple named boards around without inventing a heavy backend

## Current UX

The desktop app now provides:

- a structured sidebar with session, tools, style, and object sections
- a real tool palette instead of a single dropdown
- undo / redo / clear actions in the main window
- a status bar with current mode, object counts, and MCP connectivity
- a subtle canvas grid so the board feels like a drawing surface instead of an empty panel
- a canvas-first default layout, with session/objects/inspector/history panels hidden until requested
- one-shot contextual pane surfacing: the inspector opens the first time a drawing tool becomes active, and the objects pane opens the first time the board gains content
- an infinite-board camera with wheel zoom, middle-drag pan, and fit/reset shortcuts
- a local WebSocket runtime bridge so the GUI can subscribe to live scene updates from the MCP server

## Architecture

The app is split into four layers:

1. Desktop GUI
   `src/bin/boardmesh.rs`
   A wxDragon desktop app for local editing, selection, and visual feedback.

2. CLI
   `src/bin/boardmesh-cli.rs`
   A terminal-friendly client for the runtime command surface. It can invoke scene, session, render, and mutation commands without going through MCP.

3. MCP server
   `src/bin/boardmesh-mcp.rs`
   A stdio server exposing Boardmesh operations like adding shapes, switching sessions, exporting scenes, and rendering PNGs.

4. Shared core and runtime
   `src/lib.rs`, `src/scene.rs`, `src/storage.rs`, `src/session.rs`, `src/render.rs`, `src/runtime.rs`, `src/runtime_client.rs`
   The scene model, JSON storage, session manifests, runtime method catalog, and websocket client helpers used by the GUI, CLI, and MCP server.

There is no database and no web backend. The source of truth is the scene/session files on disk.
The live coordination path between the GUI and `boardmesh-mcp` now uses a local WebSocket runtime (`ws://127.0.0.1:9467` by default) with JSON-RPC messages. Files remain the persistence layer.

## Session-First Workflow

The preferred workflow is named sessions.

Each session has:

- `boardmesh-sessions/<session_id>.json`
- `boardmesh-sessions/<session_id>.manifest.json`

The scene file stores objects. The manifest stores metadata such as attached GUI windows and MCP server heartbeats.

By default, the installed setup uses the named session:

`default`

Raw scene-file mode still exists for compatibility, but sessions are the primary model.

## Quick Start

### Build

```bash
cargo build --bin boardmesh-cli
cargo build --bin boardmesh-mcp
cargo build --bin boardmesh --features gui
```

## CI And Release Artifacts

Boardmesh has a product-specific GitHub Actions workflow at
`.github/workflows/boardmesh-build.yml`.

The workflow:

- runs Boardmesh package tests through the reusable monorepo Rust product workflow
- builds the non-GUI wrappers `boardmesh-cli` and `boardmesh-mcp`
- builds the GUI binaries `boardmesh` and `boardmesh-layout-lab` with the `gui` feature
- uploads a release-friendly `boardmesh-linux-x86_64` artifact package containing the current Boardmesh binary names

The workflow does not publish releases, deploy Boardmesh, or depend on removed legacy compatibility paths.

### Run the GUI

```bash
cargo run --bin boardmesh --features gui -- --session default
```

If you omit `--session` and do not force a raw scene file, the app opens a session picker.

Useful flags:

```bash
cargo run --bin boardmesh --features gui -- --session default --runtime-url ws://127.0.0.1:9467
cargo run --bin boardmesh --features gui -- --session default --no-runtime
```

### Run the layout lab

```bash
cargo run --bin boardmesh-layout-lab --features gui
```

Use this when you want to test menu-driven show/hide, toolbar docking, and layout state synchronization without involving scene files, drawing tools, or MCP traffic.

### Run the MCP server

```bash
cargo run --bin boardmesh-mcp -- --session default
```

By default, the MCP server also starts the local runtime WebSocket bridge on `127.0.0.1:9467`.

Useful flags:

```bash
cargo run --bin boardmesh-mcp -- --session default --runtime-addr 127.0.0.1:9467
cargo run --bin boardmesh-mcp -- --session default --no-runtime-ws
```

### Run the CLI

```bash
cargo run --bin boardmesh-cli -- list-methods
cargo run --bin boardmesh-cli -- get_scene
cargo run --bin boardmesh-cli -- add_text --json '{"position":"120,120","text":"hello"}'
cargo run --bin boardmesh-cli -- add_text --json '{"session_id":"default","position":"120,120","text":"hello"}'
cargo run --bin boardmesh-cli -- session.list
```

The CLI talks to the local runtime websocket rather than to MCP stdio. It supports the full runtime method list, which includes:

- runtime-native methods like `session.attach`, `session.list`, `session.switch`, `scene.get`, and `scene.replace`
- the Boardmesh command surface mirrored from MCP, such as `get_universe`, `get_selection_context`, `create_canvas`, `set_active_canvas`, `select_canvases`, `assign_object_to_canvas`, `clear_object_canvas_assignment`, `add_line`, `add_text`, `duplicate_object`, `set_object_visibility`, `set_object_locked`, `move_object_to_front`, `render_scene_png`, `switch_session`, `save_scene_as`, and `get_tool_help`

Runtime session scope is per websocket client. For one-shot CLI calls, pass `"session_id"` in the JSON payload when the method should target a specific named session; long-lived clients can also use `session.attach` or `session.switch` on their own connection before sending follow-up scene updates.

Useful flags:

```bash
cargo run --bin boardmesh-cli -- get_scene --compact
cargo run --bin boardmesh-cli -- add_circle --json-file ./circle.json
cargo run --bin boardmesh-cli -- scene.replace --json '{"session_id":"default","scene":{"objects":[],"selected_object_ids":[]}}'
cargo run --bin boardmesh-cli -- session.list --runtime-url ws://127.0.0.1:9467
```

## Desktop Usage

Supported tools:

- Select
- Pen
- Line
- Rectangle
- Circle
- Triangle
- Polygon
- Fill

Useful shortcuts:

- `Delete` / `Backspace`: delete selected objects
- `Ctrl+Z`: undo
- `Ctrl+Shift+Z` or `Ctrl+Y`: redo
- Mouse wheel: zoom around the cursor
- Middle drag: pan the board
- `Ctrl+0`: reset the camera
- `Ctrl+1`: fit the board content
- `Ctrl+2`: fit the current selection
- `Enter`: finish polygon / 3-point triangle
- `Esc`: cancel the active draft

Behavior notes:

- the object list reflects current objects in the scene file
- checking items selects them in the scene
- Ctrl-click on the canvas toggles object selection
- the GUI polls the scene file and reloads changes written by MCP

## Scene Modes

### Named session mode

Recommended.

```bash
cargo run --bin boardmesh --features gui -- --session review-board
cargo run --bin boardmesh-mcp -- --session review-board
```

### Raw scene-file mode

Compatibility path for direct file workflows.

```bash
cargo run --bin boardmesh --features gui -- --scene ./boardmesh-scene.json
cargo run --bin boardmesh-mcp -- --scene ./boardmesh-scene.json
```

## MCP Usage

The MCP server exposes tools for:

- session management
- scene import/export
- drawing primitives
- selection, semantic selection context, and deletion
- PNG rendering

Common workflow:

1. `list_sessions`
2. `create_session` or `switch_session`
3. inspect the active selection with `get_selection_context` when an agent needs to resolve "this" into object kind, canvas membership, connected objects, and nearby context
4. draw with `add_rectangle`, `add_text`, `add_line`, etc.
5. export with `render_scene_png` or `export_scene_json`

If an agent needs exact payload examples, use `get_tool_help` or `get_api_docs`.

## Install In Codex

Fast path:

```bash
./install.sh
```

What `install.sh` does:

- builds the release desktop and MCP binaries
- installs them under `~/.local/share/boardmesh`
- creates the target scene file on first install if it does not exist yet
- registers the MCP server in Codex
- optionally stops installed GUI/MCP processes before replacing binaries and relaunches previously running installed GUI windows

Important flags:

- `--session ID`: install against a named session
- `--scene PATH`: force raw scene mode
- `--no-stop`: leave existing processes running
- `--no-restart-gui`: do not relaunch GUI windows after install
- `--no-codex`: skip Codex registration

Manual Codex registration example:

```bash
codex mcp add boardmesh -- \
  /home/luis/dev/luislobo/mesh/target/release/boardmesh-mcp \
  --session default
```

## Development

Product planning docs:

- `docs/PRD-001-infinite-universe-and-graph-canvases.html`

### Format

```bash
cargo fmt
```

### Tests

```bash
cargo test
```

### GUI smoke test

```bash
python3 scripts/smoke_gui.py --scenario draw
```

The smoke script builds the GUI, launches it under a virtual display, performs scripted interactions, and writes a screenshot artifact.

### Layout lab

The layout lab exists specifically for AUI and menu/toolbar debugging.

It keeps only:

- the menu bar and dock-position commands
- three toolbars
- three side panes
- a center placeholder panel
- a bottom event log

That makes it the safest place to reproduce docking and visibility bugs before changing the main Boardmesh window.

## Repository Layout

```text
src/bin/boardmesh.rs        Desktop GUI entrypoint for `boardmesh`
src/bin/boardmesh-layout-lab.rs Layout test harness entrypoint for `boardmesh-layout-lab`
src/bin/boardmesh-mcp.rs    MCP server entrypoint for `boardmesh-mcp`
src/gui_layout.rs            Shared layout state and actions
src/scene.rs                 Scene model and object logic
src/storage.rs               Scene/session storage helpers
src/session.rs               Session manifests and runtime tracking
src/render.rs                PNG rendering
scripts/smoke_gui.py         GUI smoke test
boardmesh-sessions/          Named session files
boardmesh-scene.json         Default raw-scene file
```

## Compatibility Notes

- Sessions are the preferred model.
- Raw scene files remain supported for compatibility.
- Existing MCP clients holding an old stdio process may need to be restarted after installing a new binary.
- Use `boardmesh`, `boardmesh-layout-lab`, `boardmesh-mcp`, and `boardmesh-cli`.
- `cargo test -p boardmesh` exercises the shared crate and its wrapper binaries.

## Version

Current package version: `0.2.0`
