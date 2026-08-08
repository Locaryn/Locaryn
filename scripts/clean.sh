#!/usr/bin/env bash
# Lochor clean script (Unix).
# Removes release/, target/, node_modules, and Tauri bundle outputs.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "[Lochor] Cleaning build artifacts..."

DIRS=(
    "$ROOT/release"
    "$ROOT/target"
    "$ROOT/apps/desktop/node_modules"
    "$ROOT/apps/desktop/dist"
    "$ROOT/packages-ui/core/node_modules"
    "$ROOT/packages-ui/chat/node_modules"
    "$ROOT/packages-ui/preview/node_modules"
    "$ROOT/packages-ui/terminal/node_modules"
    "$ROOT/node_modules"
)

for dir in "${DIRS[@]}"; do
    if [ -d "$dir" ]; then
        rm -rf "$dir"
        echo "  Removed $(basename "$dir")"
    fi
done

echo "[Lochor] Clean complete."
