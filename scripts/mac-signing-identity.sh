#!/usr/bin/env bash
# Create a self-signed code-signing identity for local Quicuts builds.
# Idempotent: with the identity already present, it only makes sure its
# keychain is unlocked and searchable, then exits.
#
# Why: an ad-hoc-signed .app gets a new code signature on every build, and
# TCC keys the Accessibility grant to that signature, so every rebuild of
# the bundle silently loses the grant (ADR 0006, TCC facts). A certificate
# gives the bundle a designated requirement ("identifier X and certificate
# leaf = H<hash>") that survives rebuilds, so the grant does too.
#
# This is NOT a distribution identity: Gatekeeper and notarization need an
# Apple-issued Developer ID. `just mac-build` uses one instead whenever
# APPLE_SIGNING_IDENTITY is set (docs/macos-dev.md).
#
# Where it lives: a dedicated keychain, not the login keychain. A key in the
# login keychain can only be used by codesign after
# `security set-key-partition-list`, which needs the user's login password;
# without it codesign fails with errSecInternalComponent. A keychain this
# script owns has a password it knows (a random one, kept in a 0600 file
# next to the keychain), so setup and every later unlock are unattended.
#
# The trade-off (Michael's call; docs/macos-dev.md § Signing): TCC keys the
# Accessibility grant to this certificate, and both the key and its keychain
# password are readable by any process running as you. Such a process could
# sign its own binary as com.barbowza.quicuts with this certificate and
# inherit Quicuts's grant, i.e. system-wide keyboard access, which a
# process running as you otherwise does not get without a TCC prompt. A
# Developer ID key in the login keychain behind its password would close
# that; until then, delete this keychain if the Mac is shared or untrusted
# code runs as you.
#
# Remove everything with:
#   security delete-keychain ~/Library/Keychains/quicuts-signing.keychain-db
#   rm ~/Library/Keychains/quicuts-signing.pass
set -euo pipefail

NAME="Quicuts Local Signing"
KEYCHAIN="$HOME/Library/Keychains/quicuts-signing.keychain-db"
PASSFILE="$HOME/Library/Keychains/quicuts-signing.pass"

searchable() {
    security list-keychains -d user | grep -q "quicuts-signing.keychain-db"
}
add_to_search_list() {
    # codesign finds identities through the user search list; keep the
    # existing entries (login first) and append ours.
    local current
    current=$(security list-keychains -d user | sed -e 's/^ *"//' -e 's/" *$//')
    # shellcheck disable=SC2086 # one path per line, none with spaces
    security list-keychains -d user -s $current "$KEYCHAIN"
}

if [[ -f "$KEYCHAIN" && -f "$PASSFILE" ]]; then
    security unlock-keychain -p "$(cat "$PASSFILE")" "$KEYCHAIN"
    searchable || add_to_search_list
    # Files present but no identity: an earlier run failed after
    # create-keychain. Say so instead of "ready", or mac-build fails later
    # with a less helpful codesign error.
    if ! security find-identity -p codesigning "$KEYCHAIN" | grep -q "\"$NAME\""; then
        echo "keychain exists but holds no '$NAME' identity; remove it (commands in this script's header) and rerun" >&2
        exit 1
    fi
    echo "identity '$NAME' ready"
    exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

cat >"$tmp/req.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF

# /usr/bin/openssl (LibreSSL) on purpose: Homebrew's OpenSSL 3 writes a
# PKCS#12 that `security import` cannot read without -legacy.
/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
    -config "$tmp/req.cnf" -keyout "$tmp/key.pem" -out "$tmp/cert.pem" 2>/dev/null
p12pass=$(/usr/bin/openssl rand -hex 16)
/usr/bin/openssl pkcs12 -export -name "$NAME" -passout "pass:$p12pass" \
    -inkey "$tmp/key.pem" -in "$tmp/cert.pem" -out "$tmp/id.p12"

kcpass=$(/usr/bin/openssl rand -hex 24)
(umask 077 && printf '%s' "$kcpass" >"$PASSFILE")
security create-keychain -p "$kcpass" "$KEYCHAIN"
# No auto-lock timeout and no lock on sleep (that would need -l); it locks
# at logout, and mac-build re-runs this script, which unlocks it.
security set-keychain-settings "$KEYCHAIN"
security unlock-keychain -p "$kcpass" "$KEYCHAIN"
security import "$tmp/id.p12" -k "$KEYCHAIN" -P "$p12pass" -T /usr/bin/codesign >/dev/null
# Allow Apple's signing tools to use the key without a UI prompt.
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$kcpass" "$KEYCHAIN" >/dev/null
add_to_search_list

echo "created identity '$NAME' in $KEYCHAIN"
