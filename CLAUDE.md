# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Quicuts is a cross-platform (Windows-first, macOS fast-follow) clone of PowerToys **Shortcut Guide v2**: a side-docked, app-aware panel of keyboard shortcuts shown when you hold Win/Cmd or press a hotkey chord. It imports PowerToys' YAML shortcut manifests unchanged (36 bundled in `manifests/`).

Built with **Tauri v2** — Rust backend + Svelte 5/Vite frontend.

## The two hard constraints (read before changing architecture)

1. **All system-input access lives in sidecar processes, never the main app binary.** The keyboard hook, foreground-app watcher, and taskbar reader are in `quicuts-agent-win` (a separate `.exe` shipped as a Tauri `externalBin` sidecar). This isolation is deliberate: it keeps AV/keylogger heuristics from flagging the main Quicuts app. Do not move hook/input code into `quicuts-app`.
2. **Privacy invariant: no IPC message ever carries the identity of a key the user pressed.** The only key data on the wire is the activation-chord *configuration* flowing app→agent (`ChordSpec`). The agent reports semantic transitions only (`hold_activated`, `chord_activated`, `dismissed`, `foreground_changed`), never keystrokes. Enforced by convention in `quicuts-proto`; preserve it when adding events.

## Development environment: WSL2 → Windows host

Development happens in WSL2; the app **runs on the Windows host**. Two of the four crates are Windows-only and are cross-compiled with `cargo-xwin`; the other two are platform-free and test on the Linux toolchain.

`Cargo.toml` `default-members` is limited to `quicuts-proto` + `quicuts-manifest` on purpose — so `cargo build`/`cargo test` on the Linux host don't try to build the Windows crates. The `just` targets cross-compile the Windows crates explicitly.

One-time toolchain setup (no sudo — stages clang-cl + llvm-rc from Ubuntu debs; see `docs/adr/0002`):
```bash
./scripts/setup-wsl-toolchain.sh
```

### Common commands (via `justfile`)

```bash
just test                  # fast Rust/manifest tests on the Linux toolchain (cargo test, platform-free crates only)
cargo test -p quicuts-manifest real_manifests    # run one test target
WINUSER=<you> just run     # cross-build agent+ui+app, deploy to %LOCALAPPDATA%\QuicutsDev, launch on host
just kill                  # taskkill quicuts.exe + quicuts-agent.exe on the host
just log                   # tail the host-side log
just build                 # cross-build without deploying/running
just agent                 # cross-build only the sidecar and stage it into crates/quicuts-app/binaries/
just ui                    # pnpm build the frontend
```

Set `WINUSER` to your Windows username. `deploy` must target a Windows-local path (`/mnt/c/...`), never `\\wsl$`. The app is self-contained once deployed — it can be relaunched directly from `C:\Users\<you>\AppData\Local\QuicutsDev\quicuts.exe` without rebuilding.

### Frontend HMR

`just dev-build` builds a debug exe pointed at a Vite dev server (via `conf/dev-remote.json`); `just dev-server` runs Vite on `0.0.0.0:1420` in WSL. Windows↔WSL `localhost` forwarding gives the frontend hot reload. Rust changes still require a rebuild (`just run`).

## Architecture: the four crates + UI

The **IPC protocol is the platform seam.** Every platform agent implements the same commands and events, so keep platform specifics behind the protocol — never leak them into `quicuts-app` or the UI.

- **`crates/quicuts-proto`** — the NDJSON IPC contract. `AgentCommand` (app→agent: `Configure`, `SetOverlayVisible`, `QueryTaskbar`, `SubscribeForeground`, `Ping`, `Shutdown`) and `AgentEvent` (agent→app: `Ready{caps}`, `HoldActivated`, `HoldReleased`, `ChordActivated`, `Dismissed`, `ForegroundChanged`, `Taskbar`, `Pong`, `Fatal`). Enums are `#[non_exhaustive]`; `PROTO_VERSION` is negotiated in `Ready`. `to_line`/`from_line` are the NDJSON helpers. Changing this contract touches both the agent and the app.

- **`crates/quicuts-manifest`** — the PTSG-compatible manifest engine, **platform-free and the most heavily unit-tested crate**. Pipeline: `schema.rs` (tolerant serde — `LaxBool` accepts `True`/`true`, `RawKeyToken` handles numbers/strings/null-for-`~`) → `keys.rs` (`normalize_token` → `Key` enum: `Literal`/`Vk`/`Glyph`/`UnderlinedLetter`/`TaskbarRange`/`AngleLiteral`) → `parse.rs` (`parse_manifest`, filename `<Package>.<locale>.yml` splitting, meta/taskbar section handling) → `store.rs` (`ManifestStore` layers sources — Bundled/PtsgRuntime/User, later wins whole-file; `match_foreground` by reported app identity). Per-file parse errors are logged, never fatal. `tests/real_manifests.rs` runs the whole bundled set through it — run this after any schema/parse change.

- **`crates/quicuts-agent-win`** — the Windows sidecar (only binary that installs global hooks). `hook.rs` is the **highest-risk code**: a `WH_KEYBOARD_LL` hook driving a hold/chord state machine (`Idle → WinDown → {Combo | HoldActive} → Idle`), plus PowerToys' dummy-key injection (`SendInput` VK 0xFF with a magic `dwExtraInfo`) so holding Win doesn't open the Start menu. `foreground.rs` = `SetWinEventHook` foreground watcher; `taskbar.rs` = `IUIAutomation` taskbar-rect reader; `ipc.rs`/`state.rs` = NDJSON transport + cached config atomics (the hook callback must never block on IPC). Everything is `#[cfg(windows)]`.

- **`crates/quicuts-app`** — the Tauri host. `agent.rs` supervises the sidecar (spawn, NDJSON handling, backoff restart). `engine.rs` builds the overlay view-model on each foreground change and emits `overlay://state`; the frontend is dumb-render. `overlay.rs`/`tray.rs` manage windows and the tray menu. `commands.rs` holds the `#[tauri::command]` handlers invoked from the UI. `settings.rs` (persisted to `{app_config_dir}/settings.json`, pushed live to the agent as `Configure` — no restart), `pinned.rs`, `icons.rs` (Windows icon → data-URI for the app rail). `lib.rs::run()` wires `invoke_handler` + `on_window_event`.

- **`ui/`** — Svelte 5 + Vite, **three separate windows/entry points**: `overlay.html`, `badges.html`, `settings.html`, each with its own `src/<name>/main.ts`. `lib/ipc.ts` wraps Tauri `invoke`/`listen` (with browser no-op fallbacks); `lib/types.ts` mirrors the Rust view-model types; `overlay/keys.ts` renders key glyphs. **When you add a `#[tauri::command]`, add its wrapper in `ipc.ts` and register it in `lib.rs`'s `generate_handler!`.**

## Window lifecycle gotcha

Overlay/badges/settings windows are **pre-created hidden** and reused. On close they must `prevent_close()` + `hide()` (see `on_window_event` in `lib.rs`) — do NOT let them be destroyed, or they won't reopen. Every hide of the overlay should also send `SetOverlayVisible(false)` to the agent.

## Manifest matching rules

Match foreground app by: lowercase exe name, strip one trailing `.exe`, then exact-match ∪ `"*"` wildcard manifests ∪ all `BackgroundProcess: true` manifests. Section render order: Pinned → Recommended → categories in file order → Taskbar. Ignore PowerToys' own `index.yml` — Quicuts builds its own in-memory index. Bundled manifests are MIT-licensed from PowerToys (`manifests/POWERTOYS-LICENSE`, `NOTICE.txt`).

## Status & references

Working daily-driver on Windows: hold-to-show with Win-key suppression, app-aware panels, taskbar badges, pinning, and customizations are verified on a real host. Experimental web-app title detection (hosted collections) is implemented behind a settings toggle. The macOS agent is not yet started — the IPC protocol is the seam it will plug into. ADRs: `docs/adr/0001` (why Tauri v2), `docs/adr/0002` (the no-sudo cross-toolchain), `docs/adr/0003` (hosted collections), `docs/adr/0004` (unsupported-app placeholder), `docs/adr/0005` (overlay font scaling / accessibility zoom).
