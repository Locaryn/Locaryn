#!/usr/bin/env bash
# Lochor server binaries .deb packager (Linux only).
# Builds the server binaries and creates a Debian package in release/.
#
# Usage:
#   bash scripts/build-server-deb.sh              # Enterprise remote-server (default)
#   bash scripts/build-server-deb.sh --personal   # Personal (limited) remote-server

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RELEASE_DIR="$ROOT/release"
PKG_DIR="$RELEASE_DIR/deb-pkg"

if ! command -v dpkg-deb &> /dev/null; then
    echo "[Lochor] dpkg-deb not found. This script only works on Debian/Ubuntu."
    exit 1
fi

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

VARIANT="enterprise"
if [ "$PERSONAL" -eq 1 ]; then
    VARIANT="personal"
fi

TARGET=$(rustc -vV | sed -n 's/^host: //p')
ARCH=$(dpkg --print-architecture)

# Extract workspace version from the [workspace.package] section in root Cargo.toml.
VERSION="0.1.0"
if command -v python3 &> /dev/null; then
    EXTRACTED=$(python3 - "$ROOT/Cargo.toml" <<'PY'
import re, sys
with open(sys.argv[1]) as f:
    text = f.read()
match = re.search(r'\[workspace\.package\].*?^version\s*=\s*"([^"]+)"', text, re.M | re.S)
print(match.group(1) if match else '')
PY
    )
    [ -n "$EXTRACTED" ] && VERSION="$EXTRACTED"
elif command -v awk &> /dev/null; then
    EXTRACTED=$(awk '/^\[workspace\.package\]$/{flag=1} flag && /^version\s*=/{gsub(/.*= *"|".*/,""); print; exit}' "$ROOT/Cargo.toml")
    [ -n "$EXTRACTED" ] && VERSION="$EXTRACTED"
fi

if [ "$VERSION" = "0.1.0" ]; then
    echo "[Lochor] Warning: could not extract workspace version from Cargo.toml. Using fallback $VERSION."
fi

PKG_NAME="lochor-servers-$VARIANT"
DEB_NAME="${PKG_NAME}_${VERSION}_${ARCH}.deb"

echo "[Lochor] Building $VARIANT server binaries for $TARGET"

cargo build --release -p lochor-cli -p lochor-daemon -p lochor-provider-supervisor
if [ "$PERSONAL" -eq 1 ]; then
    cargo build --release -p lochor-remote-server --no-default-features
else
    cargo build --release -p lochor-remote-server
fi

echo "[Lochor] Creating Debian package structure..."
rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/DEBIAN" "$PKG_DIR/usr/bin" "$PKG_DIR/usr/share/doc/lochor-servers"

for bin in lochor lochor-daemon lochor-remote-server lochor-supervisor; do
    SRC="$ROOT/target/release/$bin"
    if [ ! -f "$SRC" ]; then
        echo "[Lochor] Error: binary not found: $SRC"
        echo "[Lochor] Run 'bash scripts/build-servers.sh' first."
        exit 1
    fi
    cp "$SRC" "$PKG_DIR/usr/bin/"
done

# Install systemd units and default config.
mkdir -p "$PKG_DIR/lib/systemd/system" "$PKG_DIR/etc/lochor"

cat > "$PKG_DIR/lib/systemd/system/lochor-daemon.service" <<'EOF'
[Unit]
Description=Lochor local daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/lochor-daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

cat > "$PKG_DIR/lib/systemd/system/lochor-remote-server.service" <<'EOF'
[Unit]
Description=Lochor remote server
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/lochor-remote-server
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

cat > "$PKG_DIR/etc/lochor/lochor.toml" <<'EOF'
# Lochor server configuration
# See https://lochor.dev/docs for full options.

[server]
host = "127.0.0.1"
port = 7474
EOF

cat > "$PKG_DIR/DEBIAN/control" <<EOF
Package: $PKG_NAME
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: Lochor Contributors <contact@lochor.dev>
Description: Lochor server binaries ($VARIANT)
 Lochor server binaries: CLI, daemon, remote server, and provider supervisor.
 This is the $VARIANT edition.
EOF

cat > "$PKG_DIR/usr/share/doc/lochor-servers/copyright" <<EOF
Lochor server binaries ($VARIANT edition)
Copyright (C) Lochor Contributors
Licensed under Apache-2.0.
EOF

cat > "$PKG_DIR/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
systemctl daemon-reload >/dev/null 2>&1 || true
systemctl enable lochor-daemon.service lochor-remote-server.service >/dev/null 2>&1 || true
EOF

cat > "$PKG_DIR/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
systemctl stop lochor-daemon.service lochor-remote-server.service >/dev/null 2>&1 || true
systemctl disable lochor-daemon.service lochor-remote-server.service >/dev/null 2>&1 || true
EOF

chmod 755 "$PKG_DIR/DEBIAN"
chmod 755 "$PKG_DIR/DEBIAN/postinst" "$PKG_DIR/DEBIAN/prerm"
find "$PKG_DIR/usr" -type d -exec chmod 755 {} \;
find "$PKG_DIR/usr" -type f -exec chmod 755 {} \;
find "$PKG_DIR/lib" -type d -exec chmod 755 {} \; 2>/dev/null || true
find "$PKG_DIR/etc" -type d -exec chmod 755 {} \; 2>/dev/null || true

dpkg-deb --build "$PKG_DIR" "$RELEASE_DIR/$DEB_NAME"
rm -rf "$PKG_DIR"

echo "[Lochor] Debian package created: $RELEASE_DIR/$DEB_NAME"
