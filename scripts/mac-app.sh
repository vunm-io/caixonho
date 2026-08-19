#!/bin/sh
# Wrap the release binary in a minimal macOS .app bundle so caixonho opens
# like a real app — from Finder, the Dock or `open` — with no terminal
# window attached.
#
# Dev convenience only: no icon, no code signing, no notarization. Real
# packaging is its own milestone. Usage:
#
#   scripts/mac-app.sh            # build, (re)assemble bundle, open it
#   scripts/mac-app.sh --no-open  # build and assemble only
set -eu
cd "$(dirname "$0")/.."
PATH="$HOME/.cargo/bin:$PATH"
export PATH

cargo build --release

APP=target/Caixonho.app
mkdir -p "$APP/Contents/MacOS"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Caixonho</string>
    <key>CFBundleDisplayName</key><string>Caixonho</string>
    <key>CFBundleIdentifier</key><string>io.vunm.caixonho</string>
    <key>CFBundleExecutable</key><string>caixonho-gui</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
cp target/release/caixonho-gui "$APP/Contents/MacOS/"

[ "${1:-}" = "--no-open" ] || open "$APP"
echo "bundle: $APP"
