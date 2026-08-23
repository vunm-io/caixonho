#!/bin/sh
# Wrap the release binary in a minimal macOS .app bundle so caixonho opens
# like a real app — from Finder, the Dock or `open` — with no terminal
# window attached.
#
# Dev convenience only: no icon, no notarization. Real packaging is its own
# milestone. Usage:
#
#   scripts/mac-app.sh            # build, (re)assemble bundle, open it
#   scripts/mac-app.sh --no-open  # build and assemble only
#
# It signs with this machine's local identity when there is one — see
# `dev-signing-identity.sh` for what that buys and why it is not optional in
# practice.
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

# Sign with this machine's own identity when it has one. Without it the bundle
# is ad-hoc signed, whose designated requirement is the binary's own hash — so
# every rebuild is a new identity to the keychain and every "Always Allow"
# stops matching. That reads to whoever is using the app as the login password
# being demanded over and over, twice per connection and again on every switch.
IDENTITY="Caixonho Dev"
if security find-identity -v -p codesigning 2>/dev/null | grep -qF "$IDENTITY"; then
    codesign --force --sign "$IDENTITY" "$APP"
    signed="signed as $IDENTITY"
else
    signed="UNSIGNED — run scripts/dev-signing-identity.sh to stop the keychain
         asking again after every build"
fi

# Quit any instance already running before opening. `open` on a bundle that is
# already running just brings the old window forward: the build succeeds, the
# binary on disk is new, and what you are looking at is the previous one with
# nothing on screen to say so. That has cost this project a whole session once
# and most of another.
if [ "${1:-}" != "--no-open" ]; then
    osascript -e 'quit app "Caixonho"' 2>/dev/null || true
    # `quit` returns before the process is gone; opening into the gap races it.
    # Bounded, because an unbounded wait on a quit that silently did nothing is
    # a script that hangs instead of a script that reports.
    waited=0
    while pgrep -qf "$APP/Contents/MacOS/caixonho-gui"; do
        [ "$waited" -ge 50 ] && {
            echo "the running instance did not quit; close it and re-run." >&2
            exit 1
        }
        sleep 0.2
        waited=$((waited + 1))
    done
    open "$APP"
fi

# Printed so the running build can be told apart from a stale one by eye. The
# log directory names the same instant: no file for today means what is on
# screen was started earlier.
echo "bundle: $APP"
echo "binary: $(date -r "$APP/Contents/MacOS/caixonho-gui" '+%Y-%m-%d %H:%M:%S')"
echo "        $signed"
