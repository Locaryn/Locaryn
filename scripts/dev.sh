#!/usr/bin/env bash
# Lochor dev launcher (Unix).
#
# The desktop app embeds the Lochor core in-process, so it does NOT need the
# daemon to run. By default this just starts the Tauri desktop dev server.
# Pass --with-daemon to also run the local daemon (needed only for the CLI).
#
# Usage:
#   bash scripts/dev.sh                 # launch the desktop app (recommended)
#   bash scripts/dev.sh --with-daemon   # also start the daemon (for CLI testing)
#   bash scripts/dev.sh --daemon-only   # run only the daemon (foreground)
#   bash scripts/dev.sh --skip-build    # skip the daemon build

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$ROOT/tmp"
mkdir -p "$TMP_DIR"

WITH_DAEMON=0
DAEMON_ONLY=0
SKIP_BUILD=0
while [ $# -gt 0 ]; do
    case "$1" in
        --with-daemon) WITH_DAEMON=1 ;;
        --daemon-only) DAEMON_ONLY=1 ;;
        --skip-build) SKIP_BUILD=1 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
    shift
done

check_command() {
    if ! command -v "$1" &> /dev/null; then
        echo "[Lochor] $1 not found in PATH. Please install $2."
        exit 1
    fi
}

wait_health() {
    local end=$((SECONDS + ${1:-15}))
    while [ $SECONDS -lt $end ]; do
        if curl -fsS http://127.0.0.1:7474/health &> /dev/null; then
            return 0
        fi
        sleep 0.25
    done
    return 1
}

check_command cargo "Rust (https://rustup.rs/)"

DAEMON_PID=""
cleanup() {
    if [ -n "$DAEMON_PID" ]; then
        echo "[Lochor] Stopping daemon (PID $DAEMON_PID)..."
        kill "$DAEMON_PID" &> /dev/null || true
    fi
}
trap cleanup EXIT

# --- Optionally start the daemon ------------------------------------------
if [ "$WITH_DAEMON" -eq 1 ] || [ "$DAEMON_ONLY" -eq 1 ]; then
    DAEMON_EXE="$ROOT/target/debug/lochor-daemon"
    if [ "$SKIP_BUILD" -eq 0 ] && [ ! -f "$DAEMON_EXE" ]; then
        echo "[Lochor] Building daemon... (first run can take a few minutes)"
        cargo build -p lochor-daemon
    fi

    if pgrep -x "lochor-daemon" &> /dev/null; then
        echo "[Lochor] Daemon already running. Reusing it."
    elif [ -f "$DAEMON_EXE" ]; then
        echo "[Lochor] Starting daemon on http://127.0.0.1:7474 ..."
        "$DAEMON_EXE" > "$TMP_DIR/lochor-daemon.log" 2> "$TMP_DIR/lochor-daemon.err" &
        DAEMON_PID=$!
        if wait_health; then
            echo "[Lochor] Daemon ready (PID $DAEMON_PID)."
        else
            echo "[Lochor] Daemon health check failed (see tmp/lochor-daemon.log). Continuing."
        fi
    else
        echo "[Lochor] Daemon binary not found and build skipped. Continuing without it."
    fi

    if [ "$DAEMON_ONLY" -eq 1 ]; then
        echo "[Lochor] Daemon running. Press Ctrl+C to stop."
        [ -n "$DAEMON_PID" ] && wait "$DAEMON_PID"
        exit 0
    fi
fi

# --- Launch the desktop app -----------------------------------------------
check_command pnpm "pnpm (https://pnpm.io/installation)"

TAURI_BIN=""
if [ -x "$ROOT/apps/desktop/node_modules/.bin/tauri" ]; then
    TAURI_BIN="$ROOT/apps/desktop/node_modules/.bin/tauri"
elif [ -x "$ROOT/node_modules/.bin/tauri" ]; then
    TAURI_BIN="$ROOT/node_modules/.bin/tauri"
fi

if [ -z "$TAURI_BIN" ]; then
    echo "[Lochor] Tauri CLI not found. Running pnpm install..."
    pnpm install
    if [ -x "$ROOT/apps/desktop/node_modules/.bin/tauri" ]; then
        TAURI_BIN="$ROOT/apps/desktop/node_modules/.bin/tauri"
    elif [ -x "$ROOT/node_modules/.bin/tauri" ]; then
        TAURI_BIN="$ROOT/node_modules/.bin/tauri"
    fi
    [ -z "$TAURI_BIN" ] && { echo "[Lochor] Tauri CLI still missing. Check apps/desktop/package.json."; exit 1; }
fi

echo "[Lochor] Starting desktop dev (tauri dev)..."
cd "$ROOT/apps/desktop"
"$TAURI_BIN" dev
