#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$ROOT_DIR/../.." && pwd)"
TARGET_DIR="$WORKSPACE_ROOT/target"
DEFAULT_INSTALL_ROOT="${XDG_DATA_HOME:-$HOME/.local/share}/boardmesh"
INSTALL_ROOT="$DEFAULT_INSTALL_ROOT"
SCENE_PATH=""
SESSION_ID="default"
SKIP_BUILD=0
REGISTER_CODEX=1
STOP_RUNNING=1
RESTART_GUI=1

usage() {
  cat <<'EOF'
Usage: ./install.sh [options]

Options:
  --install-root PATH   Install into PATH instead of ~/.local/share/boardmesh
  --session ID          Use named session ID (default: default)
  --scene PATH          Use PATH for a raw scene file instead of named-session mode
  --skip-build          Reuse existing release binaries instead of rebuilding
  --no-codex            Do not register the MCP server in Codex
  --no-stop             Do not stop running installed processes before replacing binaries
  --no-restart-gui      Do not relaunch previously running Boardmesh GUI windows after install
  -h, --help            Show this help

Examples:
  ./install.sh
  ./install.sh --session default
  ./install.sh --scene "$HOME/.local/share/boardmesh/boardmesh-scene.json"
  ./install.sh --install-root "$HOME/opt/boardmesh"
EOF
}

stop_processes_for_path() {
  local binary_path="$1"
  local pids

  pids="$(pgrep -f "$binary_path" || true)"
  if [[ -z "$pids" ]]; then
    return 0
  fi

  echo "Stopping running process(es) for $binary_path"
  kill -TERM $pids 2>/dev/null || true

  for _ in $(seq 1 20); do
    sleep 0.25
    if ! pgrep -f "$binary_path" >/dev/null 2>&1; then
      return 0
    fi
  done

  echo "Force killing remaining process(es) for $binary_path"
  pids="$(pgrep -f "$binary_path" || true)"
  if [[ -n "$pids" ]]; then
    kill -KILL $pids 2>/dev/null || true
  fi
}

normalize_gui_command() {
  local command="$1"
  printf '%s\n' "$command"
}

capture_boardmesh_gui_commands() {
  local primary_path="$1"
  pgrep -af "^${primary_path//\//\\/} " | sed "s/^[0-9][0-9]* //" | while IFS= read -r command; do
    [[ -z "$command" ]] && continue
    normalize_gui_command "$command"
  done
}

install_binary() {
  local source_path="$1"
  local destination_path="$2"
  local temp_path="${destination_path}.new"

  install -Dm755 "$source_path" "$temp_path"
  mv -f "$temp_path" "$destination_path"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --install-root)
      INSTALL_ROOT="$2"
      shift 2
      ;;
    --scene)
      SCENE_PATH="$2"
      shift 2
      ;;
    --session)
      SESSION_ID="$2"
      shift 2
      ;;
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    --no-codex)
      REGISTER_CODEX=0
      shift
      ;;
    --no-stop)
      STOP_RUNNING=0
      shift
      ;;
    --no-restart-gui)
      RESTART_GUI=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

SESSION_ROOT="$INSTALL_ROOT/boardmesh-sessions"
SESSION_MODE=1
if [[ -n "$SCENE_PATH" ]]; then
  SESSION_MODE=0
else
  SCENE_PATH="$SESSION_ROOT/$SESSION_ID.json"
fi

BIN_DIR="$INSTALL_ROOT/bin"
DESKTOP_BIN="$BIN_DIR/boardmesh"
MCP_BIN="$BIN_DIR/boardmesh-mcp"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  echo "Building release binaries..."
  cargo build --release --bin boardmesh-mcp --manifest-path "$ROOT_DIR/Cargo.toml"
  cargo build --release --bin boardmesh --features gui --manifest-path "$ROOT_DIR/Cargo.toml"
fi

if [[ ! -x "$TARGET_DIR/release/boardmesh-mcp" ]]; then
  echo "Missing release binary: $TARGET_DIR/release/boardmesh-mcp" >&2
  exit 1
fi

if [[ ! -x "$TARGET_DIR/release/boardmesh" ]]; then
  echo "Missing release binary: $TARGET_DIR/release/boardmesh" >&2
  exit 1
fi

mkdir -p "$BIN_DIR"

GUI_COMMANDS=""
if [[ "$STOP_RUNNING" -eq 1 && "$RESTART_GUI" -eq 1 ]]; then
  GUI_COMMANDS="$(capture_boardmesh_gui_commands "$DESKTOP_BIN" || true)"
fi

if [[ "$STOP_RUNNING" -eq 1 ]]; then
  stop_processes_for_path "$MCP_BIN"
  stop_processes_for_path "$DESKTOP_BIN"
fi

install_binary "$TARGET_DIR/release/boardmesh-mcp" "$MCP_BIN"
install_binary "$TARGET_DIR/release/boardmesh" "$DESKTOP_BIN"

if [[ ! -f "$SCENE_PATH" ]]; then
  mkdir -p "$(dirname "$SCENE_PATH")"
  cp "$ROOT_DIR/boardmesh-scene.json" "$SCENE_PATH"
fi

if [[ "$REGISTER_CODEX" -eq 1 ]]; then
  if command -v codex >/dev/null 2>&1; then
    if codex mcp get boardmesh >/dev/null 2>&1; then
      codex mcp remove boardmesh >/dev/null
    fi
    if [[ "$SESSION_MODE" -eq 1 ]]; then
      codex mcp add boardmesh -- "$MCP_BIN" --sessions-dir "$SESSION_ROOT" --session "$SESSION_ID"
    else
      codex mcp add boardmesh -- "$MCP_BIN" --scene "$SCENE_PATH"
    fi
  else
    echo "codex command not found; skipping Codex MCP registration." >&2
  fi
fi

if [[ "$RESTART_GUI" -eq 1 && -n "$GUI_COMMANDS" ]]; then
  echo "Relaunching previous Boardmesh GUI window(s)..."
  while IFS= read -r command; do
    [[ -z "$command" ]] && continue
    nohup bash -lc "$command" >/dev/null 2>&1 &
  done <<< "$GUI_COMMANDS"
fi

cat <<EOF
Installed Boardmesh

Install root: $INSTALL_ROOT
Desktop app:  $DESKTOP_BIN
MCP server:   $MCP_BIN
Scene file:   $SCENE_PATH
Session mode: $( [[ "$SESSION_MODE" -eq 1 ]] && printf 'named session (%s)' "$SESSION_ID" || printf 'raw scene file' )

Run desktop app:
EOF

if [[ "$SESSION_MODE" -eq 1 ]]; then
  cat <<EOF
  $DESKTOP_BIN --sessions-dir "$SESSION_ROOT" --session "$SESSION_ID"
EOF
else
  cat <<EOF
  $DESKTOP_BIN --scene "$SCENE_PATH"
EOF
fi

cat <<EOF

If Codex registration was enabled, verify with:
  codex mcp get boardmesh

Important:
  Existing MCP stdio connections owned by already-running clients are not hot-swappable.
  Future Codex launches will use the new binary. If an already-running client keeps an old
  dead MCP session open, restart that client and retry the tool call.
EOF
