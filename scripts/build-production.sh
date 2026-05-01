#!/bin/bash
set -e

# Netswitch Production Build Script
# This script builds the daemon and the GUI, then bundles them into production installers.

echo "🚀 Starting Netswitch Production Build..."

# 1. Build the Daemon
echo "📦 Building Netswitch Daemon (Release)..."
cargo build --release --manifest-path netswitch-daemon/Cargo.toml

# Prepare Tauri Resources
mkdir -p netswitch-gui/src-tauri/bin
mkdir -p netswitch-gui/src-tauri/service

# Copy Daemon Binary
if [ -f "target/release/netswitch-daemon" ]; then
    cp target/release/netswitch-daemon netswitch-gui/src-tauri/bin/
elif [ -f "target/release/netswitch-daemon.exe" ]; then
    cp target/release/netswitch-daemon.exe netswitch-gui/src-tauri/bin/
fi

# Copy Service and Setup scripts
cp service/* netswitch-gui/src-tauri/service/
cp scripts/setup-daemon.* netswitch-gui/src-tauri/bin/

# 2. Build the Frontend and Bundle with Tauri
echo "🖥️ Building Netswitch GUI and Bundling..."
cd netswitch-gui
npm install
npm run tauri build

echo "✅ Build Complete!"
echo "Installer(s) can be found in: netswitch-gui/src-tauri/target/release/bundle/"
echo "sudo xattr -cr /Applications/netswitch-gui.app to remove quarantine on macOS if needed."