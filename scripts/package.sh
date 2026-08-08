#!/usr/bin/env bash
# Locaryn packaging helper (Linux/macOS).
# Assumes build scripts have already produced binaries. Packages them into release/.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RELEASE_DIR="$ROOT/release"
SERVERS_DIR="$RELEASE_DIR/servers"
DESKTOP_DIR="$RELEASE_DIR/desktop"

mkdir -p "$SERVERS_DIR" "$DESKTOP_DIR"

TARGET=$(rustc -vV | sed -n 's/^host: //p')
echo "[Locaryn] Packaging artifacts for $TARGET"

# Package server binaries
FOUND=0
for bin in locaryn locaryn-daemon locaryn-remote-server locaryn-supervisor; do
    if [ -f "$ROOT/target/release/$bin" ]; then
        cp "$ROOT/target/release/$bin" "$SERVERS_DIR/"
        FOUND=1
    fi
done

if [ "$FOUND" -eq 1 ]; then
    ARCHIVE_NAME="locaryn-servers-$TARGET.tar.gz"
    tar -czf "$RELEASE_DIR/$ARCHIVE_NAME" -C "$SERVERS_DIR" .
    echo "[Locaryn] Packaged server binaries: $RELEASE_DIR/$ARCHIVE_NAME"
else
    echo "[Locaryn] Warning: no server binaries found in target/release. Run build-servers.sh first."
fi

# Package desktop bundles
BUNDLE_DIR="$ROOT/target/release/bundle"
if [ -d "$BUNDLE_DIR" ]; then
    cp -r "$BUNDLE_DIR"/* "$DESKTOP_DIR/"
    echo "[Locaryn] Desktop bundles copied to $DESKTOP_DIR"
else
    echo "[Locaryn] Warning: no desktop bundles found. Run build-desktop.sh first."
fi
