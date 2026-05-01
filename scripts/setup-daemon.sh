#!/bin/bash
# Netswitch Daemon Setup Script (macOS/Linux)
# This script installs the daemon as a system service.

if [[ $EUID -ne 0 ]]; then
   echo "This script must be run as root (sudo)" 
   exit 1
fi

# Determine script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
OS="$(uname)"

# 1. Find the binary
if [ -f "$SCRIPT_DIR/netswitch-daemon" ]; then
    # Production bundle path (bin/ is same as script)
    BINARY_SOURCE="$SCRIPT_DIR/netswitch-daemon"
    SERVICE_DIR="$SCRIPT_DIR/../service"
else
    # Development path
    BINARY_SOURCE="$SCRIPT_DIR/../target/release/netswitch-daemon"
    SERVICE_DIR="$SCRIPT_DIR/../service"
fi

if [ ! -f "$BINARY_SOURCE" ]; then
    echo "Error: Daemon binary not found at $BINARY_SOURCE"
    exit 1
fi

echo "🔧 Installing Netswitch Daemon from $BINARY_SOURCE..."

# 1. Install binary
cp "$BINARY_SOURCE" /usr/local/bin/netswitch-daemon
chmod +x /usr/local/bin/netswitch-daemon

# 2. Setup Service
if [ "$OS" == "Darwin" ]; then
    echo "🍎 Setting up macOS LaunchDaemon..."
    PLIST_SOURCE="$SERVICE_DIR/com.netswitch.daemon.plist"
    cp "$PLIST_SOURCE" /Library/LaunchDaemons/
    launchctl unload /Library/LaunchDaemons/com.netswitch.daemon.plist 2>/dev/null
    launchctl load -w /Library/LaunchDaemons/com.netswitch.daemon.plist
    echo "✅ macOS Service Started"
elif [ "$OS" == "Linux" ]; then
    echo "🐧 Setting up Linux systemd service..."
    SERVICE_SOURCE="$SERVICE_DIR/netswitch-daemon.service"
    cp "$SERVICE_SOURCE" /etc/systemd/system/
    systemctl daemon-reload
    systemctl enable netswitch-daemon
    systemctl restart netswitch-daemon
    echo "✅ Linux Service Started"
fi

echo "🚀 Setup Complete!"
