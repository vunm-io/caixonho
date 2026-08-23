#!/bin/sh
# Create this machine's local code-signing identity for caixonho development.
#
# Run once per machine. Idempotent: if the identity already exists it says so
# and changes nothing.
#
#   scripts/dev-signing-identity.sh
#
# WHY THIS EXISTS
#
# `mac-app.sh` used to leave the bundle ad-hoc signed, and an ad-hoc signature
# has no identity of its own — its designated requirement is the binary's own
# hash:
#
#     designated => cdhash H"3cb78cea..."
#
# The macOS keychain binds "Always Allow" to that requirement. So every
# `cargo build` minted a new identity, every previous grant stopped matching,
# and the app asked for the login password again — twice per connection, since
# a stored credential is two keychain items (the secret access key and the
# session token), and again on every connection switch, since switching
# reopens the connection.
#
# Signed with a certificate instead, the requirement becomes the certificate:
#
#     designated => identifier "..." and certificate leaf H"..."
#
# which does not change when the binary does. One grant per keychain item, and
# it survives rebuilds.
#
# WHY A SCRIPT AND NOT KEYCHAIN ACCESS
#
# Certificate Assistant does the same job in a GUI, but what it produces lives
# on one machine and in no repository: a reinstall loses it and a second
# machine never had it. This is the same reasoning that put the rest of the
# environment in `bootstrap.sh`.
#
# The certificate is deliberately NOT portable — it is generated per machine
# rather than carried between them. It does not need to travel: keychain items
# are per-machine too, so a new machine re-enters its credentials anyway. What
# it needs is to be reproducible, which is what this file is for.
#
# WHAT IT TOUCHES
#
# The login keychain and this user's trust settings — nothing system-wide, and
# no `sudo`. `add-trusted-cert` defaults to the user domain; `-d` would be the
# admin store and is not used. macOS may still raise one authorization dialog
# to confirm the trust setting; that is expected, and it is the last one.
set -eu

NAME="Caixonho Dev"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning 2>/dev/null | grep -qF "$NAME"; then
    echo "identity already present: $NAME"
    echo "nothing to do — this script is idempotent."
    exit 0
fi

work="$(mktemp -d)"
# The private key exists as a file only inside this directory, and only until
# it has been imported. Removed on every exit path, including failure.
trap 'rm -rf "$work"' EXIT INT TERM

cat > "$work/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
EOF

# macOS ships LibreSSL, not OpenSSL. Everything used here is in both; the
# extensions go through a config file because LibreSSL's `x509` has no `-ext`.
echo "generating a self-signed code-signing certificate…"
openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
    -keyout "$work/key.pem" -out "$work/cert.pem" \
    -config "$work/cert.cnf" 2>/dev/null

# A passphrase is mandatory for PKCS#12 and this one is thrown away with the
# directory. It is passed with -P, which the man page calls insecure because
# it is visible in the process list; the exposure is this shell's lifetime,
# for a bundle holding a self-signed development certificate that is worthless
# off this machine. The alternative is a GUI passphrase prompt, which is the
# manual step this script exists to remove.
pass="$(openssl rand -hex 16)"
openssl pkcs12 -export -legacy \
    -inkey "$work/key.pem" -in "$work/cert.pem" \
    -name "$NAME" -out "$work/identity.p12" -passout "pass:$pass" 2>/dev/null \
    || openssl pkcs12 -export \
        -inkey "$work/key.pem" -in "$work/cert.pem" \
        -name "$NAME" -out "$work/identity.p12" -passout "pass:$pass"

# -T names the one program allowed to use the key without asking. Not -A,
# which would let anything on the machine sign with it.
echo "importing into the login keychain…"
security import "$work/identity.p12" \
    -k "$KEYCHAIN" -f pkcs12 -P "$pass" -T /usr/bin/codesign >/dev/null

# User domain, not admin: no sudo. Scoped to the code-signing policy, so this
# certificate is trusted to sign code and for nothing else.
echo "trusting it for code signing (macOS may ask you to confirm)…"
security add-trusted-cert -p codeSign -k "$KEYCHAIN" "$work/cert.pem"

# Verify rather than announce. `find-identity -v` lists only identities that
# are actually valid for signing, so this fails if the trust step did not take.
echo
if security find-identity -v -p codesigning | grep -qF "$NAME"; then
    security find-identity -v -p codesigning | grep -F "$NAME"
    echo
    echo "done. \`scripts/mac-app.sh\` will sign with it from now on."
    echo "The next launch asks for the keychain password once per item —"
    echo "choose **Always Allow**, and that grant now survives rebuilds."
else
    echo "the identity did not come out valid for code signing." >&2
    echo "the certificate imported but the trust setting did not take;" >&2
    echo "nothing is broken, and re-running this script is safe." >&2
    exit 1
fi
