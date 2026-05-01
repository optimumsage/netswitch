#!/bin/bash
# Netswitch Uninstaller (macOS)
# This script removes the GUI application, the daemon, and all associated files.

if [[ $EUID -ne 0 ]]; then
   echo "This script must be run as root (sudo) to remove system-level components." 
   exit 1
fi

echo "🗑️ Starting Netswitch Uninstallation..."

# 1. Stop and Remove the Background Daemon
echo "🛑 Stopping Netswitch Daemon..."
if [ -f "/Library/LaunchDaemons/com.netswitch.daemon.plist" ]; then
    launchctl unload /Library/LaunchDaemons/com.netswitch.daemon.plist 2>/dev/null
    rm "/Library/LaunchDaemons/com.netswitch.daemon.plist"
    echo "  - Removed LaunchDaemon"
fi

# 2. Remove the Daemon Binary
if [ -f "/usr/local/bin/netswitch-daemon" ]; then
    rm "/usr/local/bin/netswitch-daemon"
    echo "  - Removed daemon binary"
fi

# 3. Remove Logs
echo "📝 Cleaning up logs..."
rm -f /Library/Logs/netswitch-daemon.log
rm -f /Library/Logs/netswitch-daemon.error.log

# 4. Remove the GUI Application
# Note: Adjust name if you renamed the .app in tauri.conf.json
APP_PATH="/Applications/netswitch-gui.app"
if [ -d "$APP_PATH" ]; then
    rm -rf "$APP_PATH"
    echo "  - Removed GUI Application"
fi

# 5. Remove User Configuration and Cache
echo "🧹 Cleaning user data..."
# This finds all users and removes their local app support data for this identifier
for user_dir in /Users/*; do
    if [ -d "$user_dir/Library/Application Support/com.optimumsage.netswitch-gui" ]; then
        rm -rf "$user_dir/Library/Application Support/com.optimumsage.netswitch-gui"
    fi
done

echo "✅ Netswitch has been completely uninstalled."
