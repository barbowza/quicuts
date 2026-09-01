# Quicuts dev loop: build in WSL2, run on the Windows host.
# Requires: rustup + x86_64-pc-windows-msvc target, cargo-xwin, tauri-cli,
# clang-cl + llvm-rc on PATH (see docs/adr/0002-wsl-cross-toolchain.md), pnpm.

set shell := ["bash", "-uc"]

target := "x86_64-pc-windows-msvc"
triple := "x86_64-pc-windows-msvc"
profile := "debug"
# Set WINUSER to your Windows username (the deploy target lives under it).
winuser := env_var_or_default("WINUSER", "you")
deploy_dir := "/mnt/c/Users/" + winuser + "/AppData/Local/QuicutsDev"
export XWIN_CACHE_DIR := justfile_directory() + "/.cache/xwin"

# Default: full build + deploy + run on host.
default: run

# Fast Rust/manifest checks on the Linux toolchain (no cross-compile).
# quicuts-agent-mac builds here too — its mac deps are target-gated and the
# activation state machine is platform-free — so the Windows side gets free
# regression coverage on it.
test:
    cargo test
    cargo test -p quicuts-agent-mac

# Cross-compile the agent sidecar and stage it for the app bundle.
agent:
    cargo xwin build -p quicuts-agent-win --target {{target}}
    mkdir -p crates/quicuts-app/binaries
    cp target/{{target}}/{{profile}}/quicuts-agent.exe \
       crates/quicuts-app/binaries/quicuts-agent-{{triple}}.exe

# Build the frontend.
ui:
    cd ui && pnpm install --prefer-offline && pnpm build

# Full app build (agent + ui + app exe). No installer (that's CI-only).
build: agent ui
    cargo xwin build -p quicuts-app --target {{target}}

# Copy exe + sidecar + manifests to a Windows-local folder (NOT \\wsl$).
deploy: build
    mkdir -p "{{deploy_dir}}" "{{deploy_dir}}/manifests"
    cp target/{{target}}/{{profile}}/quicuts.exe "{{deploy_dir}}/"
    cp crates/quicuts-app/binaries/quicuts-agent-{{triple}}.exe "{{deploy_dir}}/quicuts-agent.exe"
    cp manifests/*.yml "{{deploy_dir}}/manifests/"
    cp manifests/*.png "{{deploy_dir}}/manifests/" 2>/dev/null || true

# Launch on the Windows host.
run: deploy
    cmd.exe /c start "" "C:\\Users\\{{winuser}}\\AppData\\Local\\QuicutsDev\\quicuts.exe"

# Kill any running instances on the host.
kill:
    -taskkill.exe /IM quicuts.exe /F 2>/dev/null
    -taskkill.exe /IM quicuts-agent.exe /F 2>/dev/null

# Frontend HMR: Vite in WSL + a debug exe pointed at it (localhost forwards
# WSL<->Windows). Rebuild the exe with the dev-remote config once, then run.
# The overlay goes in via TAURI_CONFIG (a JSON *string*, json-patch-merged by
# tauri-build) — `cargo --config` is cargo's own flag and only parses TOML.
dev-build: agent
    TAURI_CONFIG="$(cat crates/quicuts-app/conf/dev-remote.json)" \
        cargo xwin build -p quicuts-app --target {{target}}
    mkdir -p "{{deploy_dir}}"
    cp target/{{target}}/{{profile}}/quicuts.exe "{{deploy_dir}}/"
    cp crates/quicuts-app/binaries/quicuts-agent-{{triple}}.exe "{{deploy_dir}}/quicuts-agent.exe"

# Run the Vite dev server (leave running in one terminal).
dev-server:
    cd ui && pnpm dev

# Tail the host-side log.
log:
    tail -f "{{deploy_dir}}/logs/Quicuts.log" 2>/dev/null || \
      tail -f "/mnt/c/Users/{{winuser}}/AppData/Roaming/com.barbowza.quicuts/logs/Quicuts.log"

# --- macOS (native build on the Mac; see docs/macos-dev.md) ---

# The sidecar must be named for the *host* triple tauri-build looks up, so it
# is derived, not hardcoded — Apple silicon and Intel both build here.
# Computed inside the recipes so a machine without rustc can still parse this
# justfile and run the Windows recipes.

# Rust/manifest/state-machine tests on the native toolchain. Includes
# quicuts-app, which the Windows `just test` cannot run (it does not
# cross-compile on the Linux host) — so the engine's rail-selection tests
# only ever execute here.
mac-test:
    cargo test -p quicuts-proto -p quicuts-manifest -p quicuts-agent-mac -p quicuts-app

# Build the agent sidecar and stage it where tauri-build expects it.
# (tauri-build re-copies the staged file over target/debug/quicuts-agent on
# every app build, so restaging here is what keeps the dev sidecar fresh.)
mac-agent:
    #!/usr/bin/env bash
    set -euo pipefail
    triple=$(rustc -vV | sed -n 's/^host: //p')
    cargo build -p quicuts-agent-mac
    mkdir -p crates/quicuts-app/binaries
    cp target/debug/quicuts-agent "crates/quicuts-app/binaries/quicuts-agent-$triple"

# Release build of the sidecar, staged the same way. `cargo tauri build` is a
# release build, so staging the debug binary would ship it inside the .app.
mac-agent-release:
    #!/usr/bin/env bash
    set -euo pipefail
    triple=$(rustc -vV | sed -n 's/^host: //p')
    cargo build -p quicuts-agent-mac --release
    mkdir -p crates/quicuts-app/binaries
    cp target/release/quicuts-agent "crates/quicuts-app/binaries/quicuts-agent-$triple"

# Build the frontend.
mac-ui:
    cd ui && pnpm install --prefer-offline && pnpm build

# Full .app bundle build (permission-realistic; TCC attributes to the app).
mac-build: mac-agent-release mac-ui
    cd crates/quicuts-app && cargo tauri build --config conf/macos.json

# Dev run from the terminal (TCC attributes to the terminal app).
mac-run: mac-agent mac-ui
    @echo "TIP: permission checkpoints can be removed — see docs/macos-dev.md § Autonomous mode"
    cd crates/quicuts-app && cargo tauri dev --config conf/macos.json

# Tail the mac-side log.
mac-log:
    tail -f "$HOME/Library/Logs/com.barbowza.quicuts/Quicuts.log"

# Subcommands: hold [ms] | holddown | cmdup | chord | esc | help. The only way
# to drive activation programmatically — the agent reads modifier direction
# from the device-dependent NX_DEVICE* flag bits, which AppleScript never sets;
# the script's header and docs/macos-dev.md explain why. Compiled on the fly by
# the `swift` interpreter, with no cargo involvement, so it cannot affect any
# build.
# A stray Esc or keystroke lands in whatever is frontmost and can dismiss a
# dialog or replace an editor selection, losing unsaved work.
# DANGER: synthetic input goes to the FOCUSED app, not Quicuts — idle machine only.
mac-input *ARGS:
    swift scripts/mac-synthetic-input.swift {{ARGS}}

# The script decodes the two traps that silently corrupt a hand-read: the
# AXMenuItemCmdModifiers command bit is INVERTED (0x8 = no cmd), and 0x10 is
# fn/Globe, which also marks the macOS window-tiling items the system injects
# into every app's Window menu. Those rows are tagged `system-fn`; menus also
# contain user data (saved arrangements, profiles). Read its header first.
# Read-only (Accessibility API) — unlike mac-input it posts no events.
# Dump a RUNNING app's menu shortcuts as TSV: `just mac-menus iTerm`.
mac-menus *ARGS:
    swift scripts/mac-menu-shortcuts.swift {{ARGS}}

# --- Collaboration (docs/collaboration.md, two-agent-flow skill) -------------

# The remote is the claim board and it moves; read this before starting work
# and again before merging.
# Who holds which branch, what is open, what CI is doing.
status:
    @git fetch --prune --quiet origin
    @echo "== local =="
    @printf '  branch: %s\n' "$(git rev-parse --abbrev-ref HEAD)"
    @printf '  identity: %s <%s>\n' "$(git config user.name || echo UNSET)" "$(git config user.email || echo UNSET)"
    @git status --short | head -20 | sed 's/^/  /' || true
    @printf '  vs origin/main: %s ahead, %s behind\n' \
        "$(git rev-list --count origin/main..HEAD 2>/dev/null || echo ?)" \
        "$(git rev-list --count HEAD..origin/main 2>/dev/null || echo ?)"
    @echo "== claim board (remote branches) =="
    @git ls-remote --heads origin | sed 's/.*refs\/heads\//  /'
    @echo "== open PRs =="
    @gh pr list --state open 2>/dev/null | sed 's/^/  /' || echo "  (gh unavailable)"
    @echo "== CI on main =="
    @gh run list --branch main --limit 3 2>/dev/null | sed 's/^/  /' || true

# Terminates the running app, deploys, relaunches, reports what he now has, so
# he never tests a stale build.
# Put a fresh instance in front of Michael before asking him to test.
stage: build
    @just kill
    @mkdir -p "{{deploy_dir}}" "{{deploy_dir}}/manifests"
    @cp target/{{target}}/{{profile}}/quicuts.exe "{{deploy_dir}}/"
    @cp crates/quicuts-app/binaries/quicuts-agent-{{triple}}.exe "{{deploy_dir}}/quicuts-agent.exe"
    @cp manifests/*.yml "{{deploy_dir}}/manifests/"
    @-cp manifests/*.png "{{deploy_dir}}/manifests/" 2>/dev/null || true
    @cmd.exe /c start "" "C:\\Users\\{{winuser}}\\AppData\\Local\\QuicutsDev\\quicuts.exe"
    @echo "staged and relaunched:"
    @printf '  commit:  %s\n' "$(git log --format='%h %s' -1 | cut -c1-64)"
    @printf '  branch:  %s\n' "$(git rev-parse --abbrev-ref HEAD)"
    @echo "  now give Michael a numbered plan with an expected result per step."
