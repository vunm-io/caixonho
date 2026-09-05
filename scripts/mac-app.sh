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
# practice. With no identity — CI, or a fresh machine — it still signs the
# **bundle** ad-hoc, and that is not optional either: see the note below on
# what Gatekeeper does to a bundle that was never signed as a bundle.
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
# An Apple-issued identity outranks the local one when both exist: it is the
# only kind whose signature carries a TeamIdentifier, and the keychain's
# partition list keys on that — so grants survive rebuilds with zero prompts.
# The local self-signed identity keys the partition on the build's own hash
# instead, which costs one password per stored credential after each rebuild
# (measured 2026-08-23; the why is in dev-signing-identity.sh).
identities="$(security find-identity -v -p codesigning 2>/dev/null)"
if printf '%s' "$identities" | grep -q "Apple Development"; then
    IDENTITY="Apple Development"
elif printf '%s' "$identities" | grep -qF "Caixonho Dev"; then
    IDENTITY="Caixonho Dev"
else
    IDENTITY=""
fi
if [ -n "$IDENTITY" ]; then
    # Diagnosed rather than left to `set -e`, because the state this fails in
    # is worse than a failed build and does not look like one. `--force` has
    # already replaced the old signature by the time it gives up, so what is
    # left on disk is the linker's *ad-hoc* signature — a bundle that opens
    # perfectly well and asks for the keychain password on every single
    # connection, because every grant is bound to the signature it was given
    # under. Seen 2026-08-26: the dialog names `codesign`, so it reads as the
    # app demanding a password at launch, which is not what is happening.
    # An interrupted codesign leaves a `.cstemp` inside the bundle, and every
    # later attempt then dies with "invalid or unsupported format for
    # signature" naming that file — a message that sends you looking at the
    # signature rather than at the leftovers. Seen 2026-08-26, after a signing
    # run was cut off waiting for the keychain.
    find "$APP" -name '*.cstemp' -delete 2>/dev/null || true
    if ! codesign --force --sign "$IDENTITY" "$APP"; then
        cat >&2 <<MSG

codesign could not sign with "$IDENTITY", and $APP is now ad-hoc signed.
Do not open it: an ad-hoc bundle asks for the keychain on every connection.

The usual cause is the login keychain being locked — after sleep, or a fresh
login. macOS asks in a dialog that names *codesign*, not this app. Clicking
"Always Allow" there is the one-click fix; it lets codesign use the key from
then on. Otherwise unlock the keychain and run this script again:

    security unlock-keychain ~/Library/Keychains/login.keychain-db

If the dialog returns on every build, grant codesign the key once. It asks for
the password itself — never put it on the command line:

    security set-key-partition-list -S apple-tool:,apple:,codesign: \\
        -s -l identity ~/Library/Keychains/login.keychain-db

MSG
        exit 1
    fi
    signed="signed as $IDENTITY"
else
    # No identity, so ad-hoc — but the *bundle*, not only the binary. The
    # linker already ad-hoc signs the bare Mach-O, and until 2026-09-05 this
    # branch stopped there, which left a bundle whose only signature was made
    # before the bundle existed: `Info.plist=not bound`, `Sealed
    # Resources=none`. Gatekeeper reads that as a bundle somebody altered after
    # signing and shows **"Caixonho is damaged and can't be opened. You should
    # move it to the Trash."** — with no Open Anyway anywhere. Both public
    # betas shipped like this, and the release notes described a dialog nobody
    # downloading them could have seen.
    #
    # Signing the bundle seals `Info.plist` and writes
    # `_CodeSignature/CodeResources`; the same download then gets the ordinary
    # "could not verify the developer" dialog, whose answer really is
    # Privacy & Security → Open Anyway. Not `--deep`: one code object here, and
    # Apple's guidance is to sign it, not recurse.
    codesign --force --sign - "$APP"
    signed="ad-hoc signed as a bundle — opens via Open Anyway; run
         scripts/dev-signing-identity.sh to stop the keychain asking again after
         every build"
fi

# Either branch has to leave a bundle Gatekeeper can evaluate at all. This is
# the check that would have been red for both betas: it is the exact
# assessment Gatekeeper makes, and "code has no resources but signature
# indicates they must be present" is the message behind "damaged".
codesign --verify --deep --strict --verbose=2 "$APP"

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
