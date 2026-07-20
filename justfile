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
test:
    cargo test

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
dev-build: agent
    cargo xwin build -p quicuts-app --target {{target}} \
        --config crates/quicuts-app/conf/dev-remote.json
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
