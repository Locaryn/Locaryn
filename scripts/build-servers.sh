#!/usr/bin/env bash
# Lochor server binaries release build (Linux/macOS).
# Builds CLI, daemon, remote-server, and provider-supervisor in release mode.
#
# Usage:
#   bash scripts/build-servers.sh              # Enterprise remote-server (default)
#   bash scripts/build-servers.sh --personal   # Personal (limited) remote-server

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RELEASE_DIR="$ROOT/release"
SERVERS_DIR="$RELEASE_DIR/servers"

mkdir -p "$SERVERS_DIR"

if ! command -v cargo &> /dev/null; then
    echo "[Lochor] cargo not found in PATH. Please install Rust: https://rustup.rs/"
    exit 1
fi

PERSONAL=${PERSONAL:-0}
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
echo "[Lochor] Building $VARIANT server binaries for $TARGET"

# Build common binaries.
cargo build --release -p lochor-cli -p lochor-daemon -p lochor-provider-supervisor

# Build remote-server with the chosen feature set.
if [ "$PERSONAL" -eq 1 ]; then
    cargo build --release -p lochor-remote-server --no-default-features
else
    cargo build --release -p lochor-remote-server
fi

echo "[Lochor] Copying server binaries..."
for bin in lochor lochor-daemon lochor-remote-server lochor-supervisor; do
    if [ -f "$ROOT/target/release/$bin" ]; then
        cp "$ROOT/target/release/$bin" "$SERVERS_DIR/"
    else
        echo "[Lochor] Warning: binary not found: $bin"
    fi
done

ARCHIVE_NAME="lochor-servers-$VARIANT-$TARGET.tar.gz"
tar -czf "$RELEASE_DIR/$ARCHIVE_NAME" -C "$SERVERS_DIR" .

echo "[Lochor] Server binaries built and packaged:"
echo "  Directory: $SERVERS_DIR"
echo "  Archive:   $RELEASE_DIR/$ARCHIVE_NAME"
