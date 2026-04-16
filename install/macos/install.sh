#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
INSTALL_DIR="/Library/Application Support/HydroServerSDL"
PLIST_PATH="/Library/LaunchDaemons/com.hydroserver.sdl.plist"
SERVICE_BINARY="$REPO_ROOT/target/release/sdl-service"

if [ "$EUID" -ne 0 ]; then
    echo "Must be run as root (use sudo)."
    exit 1
fi

if [ ! -x "$SERVICE_BINARY" ]; then
    echo "Missing release service binary at $SERVICE_BINARY"
    echo "Build it first with: cargo build -p sdl-service --release"
    exit 1
fi

mkdir -p "$INSTALL_DIR/bin" "$INSTALL_DIR/logs"
cp "$SERVICE_BINARY" "$INSTALL_DIR/bin/sdl-service"
chown -R root:wheel "$INSTALL_DIR"
chmod 755 "$INSTALL_DIR/bin/sdl-service"

if launchctl print system/com.hydroserver.sdl >/dev/null 2>&1; then
    launchctl bootout system/com.hydroserver.sdl
fi

cp "$SCRIPT_DIR/com.hydroserver.sdl.plist" "$PLIST_PATH"
chown root:wheel "$PLIST_PATH"
chmod 644 "$PLIST_PATH"

launchctl bootstrap system "$PLIST_PATH"
launchctl enable system/com.hydroserver.sdl
launchctl kickstart -k system/com.hydroserver.sdl
echo "Installed and started."
