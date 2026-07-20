#!/usr/bin/env bash
# One-time WSL2 toolchain setup for building Quicuts (Windows target).
#
# The clean path is `sudo apt install lld clang llvm` — if you have sudo,
# do that and skip the deb-extraction section. This script provides the
# no-sudo fallback used during initial development (see
# docs/adr/0002-wsl-cross-toolchain.md).
set -euo pipefail

# 1. Rust + Windows target + cargo tools.
if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin tauri-cli || true

# 2. clang-cl + llvm-rc (needed by cc-rs and tauri-winres when cross-compiling).
if command -v clang-cl >/dev/null && command -v llvm-rc >/dev/null; then
  echo "clang-cl and llvm-rc already present."
  exit 0
fi

if command -v sudo >/dev/null && sudo -n true 2>/dev/null; then
  sudo apt-get update -qq
  sudo apt-get install -y lld clang llvm
  exit 0
fi

echo ">> No sudo: staging clang-cl + llvm-rc from apt debs into ~/.cargo/bin"
LIBDIR="$HOME/.local/lib/quicuts-llvm"
TMP="$(mktemp -d)"
mkdir -p "$LIBDIR"
pushd "$TMP" >/dev/null
apt-get download llvm-14 libllvm14t64 clang-14 libclang-cpp14t64 libclang-common-14-dev
for d in *.deb; do dpkg-deb -x "$d" tree; done
# libs
cp tree/usr/lib/x86_64-linux-gnu/libLLVM-14.so "$LIBDIR/"
ln -sf "$LIBDIR/libLLVM-14.so" "$LIBDIR/libLLVM-14.so.1"
cp tree/usr/lib/x86_64-linux-gnu/libclang-cpp.so.14 "$LIBDIR/"
# real binaries
cp tree/usr/lib/llvm-14/bin/llvm-rc "$LIBDIR/llvm-rc.real"
cp tree/usr/lib/llvm-14/bin/clang   "$LIBDIR/clang.real"
chmod +x "$LIBDIR"/*.real
popd >/dev/null
rm -rf "$TMP"

mkexec() { # name  extra-args
  cat > "$HOME/.cargo/bin/$1" <<EOF
#!/bin/sh
export LD_LIBRARY_PATH="$LIBDIR:\$LD_LIBRARY_PATH"
exec "$LIBDIR/$2" $3 "\$@"
EOF
  chmod +x "$HOME/.cargo/bin/$1"
}
mkexec llvm-rc  llvm-rc.real ""
mkexec clang    clang.real   ""
mkexec clang-cl clang.real   "--driver-mode=cl"

echo ">> Done. Verify: clang-cl --version && llvm-rc --version"
