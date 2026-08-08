#!/usr/bin/env bash
# Locaryn desktop app release build (Linux/macOS).
# Builds the Tauri desktop app and copies bundles to release/desktop/.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RELEASE_DIR="$ROOT/release"
DESKTOP_DIR="$RELEASE_DIR/desktop"

mkdir -p "$DESKTOP_DIR"

if ! command -v pnpm &> /dev/null; then
    echo "[Locaryn] pnpm not found in PATH. Please install pnpm (https://pnpm.io/installation)."
    exit 1
fi

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
