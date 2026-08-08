#!/usr/bin/env bash
# Locaryn full release build (Linux/macOS).
# Builds all server binaries and the desktop app, then packages artifacts into release/.
#
# Usage:
#   bash scripts/build-all.sh              # Enterprise remote-server (default)
#   bash scripts/build-all.sh --personal   # Personal (limited) remote-server

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RELEASE_DIR="$ROOT/release"
SERVERS_DIR="$RELEASE_DIR/servers"
DESKTOP_DIR="$RELEASE_DIR/desktop"

mkdir -p "$RELEASE_DIR" "$SERVERS_DIR" "$DESKTOP_DIR"

check_command() {
    if ! command -v "$1" &> /dev/null; then
        echo "[Locaryn] $1 not found in PATH. Please install $2."
        exit 1
    fi
}

check_command cargo "Rust (https://rustup.rs/)"
check_command pnpm "pnpm (https://pnpm.io/installation)"

PERSONAL=${PERSONAL:-0}
SKIP_SERVERS=${SKIP_SERVERS:-0}
SKIP_DESKTOP=${SKIP_DESKTOP:-0}

while [ $# -gt 0 ]; do
    case "$1" in
        --personal) PERSONAL=1 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
    shift
done

TARGET=$(rustc -vV | sed -n 's/^host: //p')
VARIANT="enterprise"
if [ "$PERSONAL" -eq 1 ]; then
    VARIANT="personal"
fi
echo "[Locaryn] Building $VARIANT release for $TARGET"

if [ "$SKIP_SERVERS" -ne 1 ]; then
    echo "[Locaryn] Building server binaries..."
    cargo build --release -p locaryn-cli -p locaryn-daemon -p locaryn-provider-supervisor

    if [ "$PERSONAL" -eq 1 ]; then
        cargo build --release -p locaryn-remote-server --no-default-features
    else
        cargo build --release -p locaryn-remote-server
    fi

    echo "[Locaryn] Copying server binaries..."
    for bin in locaryn locaryn-daemon locaryn-remote-server locaryn-supervisor; do
        if [ -f "$ROOT/target/release/$bin" ]; then
            cp "$ROOT/target/release/$bin" "$SERVERS_DIR/"
        else
            echo "[Locaryn] Warning: binary not found: $bin"
        fi
    done

    ARCHIVE_NAME="locaryn-servers-$VARIANT-$TARGET.tar.gz"
    tar -czf "$RELEASE_DIR/$ARCHIVE_NAME" -C "$SERVERS_DIR" .
    echo "[Locaryn] Packaged server binaries: $RELEASE_DIR/$ARCHIVE_NAME"
fi

if [ "$SKIP_DESKTOP" -ne 1 ]; then
    echo "[Locaryn] Building desktop app..."
    cd "$ROOT/apps/desktop"
    pnpm install || exit $?
    pnpm tauri build || exit $?

    BUNDLE_DIR="$ROOT/target/release/bundle"
    if [ -d "$BUNDLE_DIR" ]; then
        cp -r "$BUNDLE_DIR"/* "$DESKTOP_DIR/"
        echo "[Locaryn] Desktop bundles copied to $DESKTOP_DIR"
    else
        echo "[Locaryn] Warning: desktop bundle directory not found: $BUNDLE_DIR"
    fi
fi

echo "[Locaryn] Release build complete. Artifacts in $RELEASE_DIR"
