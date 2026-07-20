# ADR 0002 — WSL2 → Windows cross-build toolchain

Status: accepted (2026-07-02)

## Context

We build on WSL2 (Linux) and run on the Windows host. Rust cross-compiles to
`x86_64-pc-windows-msvc` via **cargo-xwin**, which downloads the MSVC CRT/SDK
and links with rust-lld — no Visual Studio needed. This works out-of-the-box
for pure-Rust crates (verified: `quicuts-proto`, `quicuts-agent-win`).

The **main app** additionally needs two host tools:
1. `clang-cl` — a transitive dependency compiles C via cc-rs.
2. `llvm-rc` — `tauri-winres` compiles the Windows resource (icon + app
   manifest); without it the build panics `NotAttempted("llvm-rc")`, and a
   missing app manifest breaks DPI awareness → wrong overlay geometry.

The clean install is `sudo apt install lld clang llvm`.

## Decision

Where sudo is available, `apt install lld clang llvm`. Our dev box had no
sudo, so we stage the tools **without root** by extracting Ubuntu debs:

- `apt-get download llvm-14 libllvm14t64 clang-14 libclang-cpp14t64
  libclang-common-14-dev` (no root needed)
- `dpkg-deb -x` each, copy `libLLVM-14.so` (symlink → `.so.1`),
  `libclang-cpp.so.14`, and the `clang`/`llvm-rc` binaries into
  `~/.local/lib/quicuts-llvm/`
- put shell wrappers on PATH (`~/.cargo/bin/`) that set `LD_LIBRARY_PATH` and
  exec the real binary; `clang-cl` = `clang --driver-mode=cl`

`scripts/setup-wsl-toolchain.sh` automates both paths. Pin
`XWIN_CACHE_DIR=<repo>/.cache/xwin` for reproducible SDK caching.

## Consequences

- Fully offline-reproducible builds from WSL with no admin rights.
- LLVM 14 is older than the host clang would be, but only used for `.rc`
  compilation and C shims — version-insensitive here.
- NSIS installer packaging (cross-built from Linux) is officially experimental;
  deferred to Windows-side CI (`windows-latest`). Dev uses `--no-bundle` exes.

## Verification (Spike 0, on the Windows host)

`just deploy && cmd.exe start quicuts.exe` produced a live `quicuts.exe`
(WebView2 up) + `quicuts-agent.exe`, and the log showed `loaded 34 bundled
manifests (0 failed)` and `agent ready (proto 1)` — i.e. the cross-built exe
boots, resources embed, the sidecar spawns, the WH_KEYBOARD_LL hook installs,
and the NDJSON handshake works. Interactive keyboard behavior (hold-to-show,
Win suppression, Win+E passthrough) still needs manual on-host testing.
