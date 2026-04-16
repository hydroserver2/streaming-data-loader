#!/bin/bash
set -euo pipefail

if [ "$EUID" -ne 0 ]; then
    echo "Must be run as root (use sudo)."
    exit 1
fi

launchctl bootout system/com.hydroserver.sdl 2>/dev/null || true
rm -f /Library/LaunchDaemons/com.hydroserver.sdl.plist
# Leave config dir intact by default — user can rm manually.
echo "Uninstalled."
