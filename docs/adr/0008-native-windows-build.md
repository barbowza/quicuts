# ADR 0008 — Native Windows build path, alongside the WSL cross-build

Status: accepted (2026-10-13). Amends ADR 0002: the WSL cross-build is now one
of two supported Windows build paths, not the only one.

## Context

Until now the Windows build existed only as a WSL2 cross-compile (`cargo xwin`,
ADR 0002), and every Windows recipe assumed bash, `/mnt/c` and `cmd.exe`.
Michael wanted a second path, built natively on the Windows host by a new
session, pro-win-quicuts, in `C:\Users\pcpro\projects\quicuts\quicuts` (issue
#43). He decided four things up front: pro-win-quicuts becomes the lead; both
Prometheus clones share the `pro` journal; **both builds deploy to the same
`%LOCALAPPDATA%\QuicutsDev`**, so only one Quicuts (one keyboard hook) ever
runs and settings stay shared; and the WSL path stays.

## Decision

**One justfile, gated by OS attribute, with the same recipe names on both
paths.**

- `set windows-shell := ["C:/Program Files/Git/bin/bash.exe", "-uc"]`, so
  recipes are bash on every OS and most bodies need no translation.
- The WSL recipes that touch `/mnt/c`, `cmd.exe`, `taskkill.exe` or
  `cargo xwin` (`test`, `agent`, `build`, `deploy`, `run`, `kill`,
  `dev-build`, `log`, `stage`) are marked `[unix]`. Each has a same-named
  `[windows]` twin. `[unix]` rather than `[linux]`, so macOS resolves exactly
  as before. `ui`, `dev-server`, `status` and the `mac-*` recipes are shared
  and unchanged.
- Native builds are host builds: output in `target/debug`, `cargo build -p …`
  with no `--target`, no tauri-cli. The sidecar is staged under the same
  `quicuts-agent-x86_64-pc-windows-msvc.exe` name.
- `deploy_dir` resolves from `%LOCALAPPDATA%` natively and from `WINUSER` +
  `/mnt/c` under WSL — the same folder either way.
- Process control natively goes through `powershell -NoProfile -Command`
  (`Stop-Process`, `Start-Process`), never `taskkill /IM` from Git bash, which
  rewrites `/IM` as a path. The native `deploy` depends on `kill`, because a
  running exe cannot be overwritten; `kill` succeeds when nothing runs and
  waits for the processes to exit, so the exe locks are gone before the copy.
- `.gitattributes` forces LF (`* text=auto eol=lf`), marks images binary, and
  leaves the CRLF third-party `manifests/POWERTOYS-LICENSE` untouched
  (`-text`). Clones are also made with `core.autocrlf=false`.

## Consequences

- **The native `just test` covers every crate**, including `quicuts-app` (the
  engine's rail-selection tests), which WSL cannot build. It depends on
  `agent` and `ui` because `quicuts-app`'s build script needs the staged
  sidecar and `ui/dist`.
- Two Windows paths now exist, and each must keep working. A recipe change on
  one side needs its twin changed too. CI still runs the Linux recipes; a
  `windows-latest` job for the native ones is a follow-up.
- `target/` is per-clone. The two clones share only GitHub, the journal and
  the deploy folder, so a deploy from either replaces the other's build.
- Bitdefender, the antivirus on this host, needs exceptions for the native
  clone's `target\` and for `QuicutsDev`. Without them, debug exes are
  quarantined after linking and the next link fails with `LNK1104`.
- A build or deploy started from the Claude desktop app lands in a redirected
  `%LOCALAPPDATA%`. Builds run from the Claude Code CLI.
- Deferred: pinning a `rust-toolchain.toml` (it affects WSL, the Mac and CI
  too), and the same-day add/add conflict when both clones write `diary/pro`.

## Verification (2026-10-13, on Prometheus)

`just test` natively: 91 tests across all five crates pass. `just stage`
built, deployed to `C:\Users\pcpro\AppData\Local\QuicutsDev` and launched
`quicuts.exe` + `quicuts-agent.exe`. The log (in
`%LOCALAPPDATA%\com.barbowza.quicuts\logs`, shared with the WSL build's runs)
shows `loaded 38 bundled manifests (0 failed)` and `agent ready (proto 1)`.
`git add --renormalize .` with the new `.gitattributes` changes no file.
