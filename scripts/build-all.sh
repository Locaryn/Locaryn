#!/usr/bin/env bash
# Lochor full release build (Linux/macOS).
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
        echo "[Lochor] $1 not found in PATH. Please install $2."
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
echo "[Lochor] Building $VARIANT release for $TARGET"

if [ "$SKIP_SERVERS" -ne 1 ]; then
    echo "[Lochor] Building server binaries..."
    cargo build --release -p lochor-cli -p lochor-daemon -p lochor-provider-supervisor

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
    echo "[Lochor] Packaged server binaries: $RELEASE_DIR/$ARCHIVE_NAME"
fi

if [ "$SKIP_DESKTOP" -ne 1 ]; then
    echo "[Lochor] Building desktop app..."
    cd "$ROOT/apps/desktop"
    pnpm install || exit $?
    pnpm tauri build || exit $?

    BUNDLE_DIR="$ROOT/target/release/bundle"
    if [ -d "$BUNDLE_DIR" ]; then
        cp -r "$BUNDLE_DIR"/* "$DESKTOP_DIR/"
        echo "[Lochor] Desktop bundles copied to $DESKTOP_DIR"
    else
        echo "[Lochor] Warning: desktop bundle directory not found: $BUNDLE_DIR"
    fi
fi

echo "[Lochor] Release build complete. Artifacts in $RELEASE_DIR"
