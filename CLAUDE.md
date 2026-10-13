# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Quicuts is a cross-platform (Windows-first, macOS fast-follow) clone of PowerToys **Shortcut Guide v2**: a side-docked, app-aware panel of keyboard shortcuts shown when you hold Win/Cmd or press a hotkey chord. It imports PowerToys' YAML shortcut manifests unchanged (36 bundled in `manifests/`; macOS-specific ones live in `manifests-mac/`).

Built with **Tauri v2** — Rust backend + Svelte 5/Vite frontend.

## The two hard constraints (read before changing architecture)

1. **All system-input access lives in sidecar processes, never the main app binary.** Each platform gets its own agent binary, shipped as a Tauri `externalBin` sidecar. On Windows that's `quicuts-agent-win` (keyboard hook, foreground-app watcher, taskbar reader); the macOS agent follows the same rule. This isolation is deliberate: on Windows it keeps AV/keylogger heuristics from flagging the main Quicuts app, and on both platforms it buys crash isolation plus one shared supervision path in `agent.rs`. Do not move hook/tap/input code into `quicuts-app`.
2. **Privacy invariant: no IPC message ever carries the identity of a key the user pressed.** The only key data on the wire is the activation-chord *configuration* flowing app→agent (`ChordSpec`). The agent reports semantic transitions only (`hold_activated`, `chord_activated`, `dismissed`, `foreground_changed`), never keystrokes. Enforced by convention in `quicuts-proto`; preserve it when adding events.

## Development environments

Quicuts targets two platforms, and Windows has two build paths: a WSL2 cross-build and a native one from PowerShell. They share one set of `just` recipe names, with `[unix]`/`[windows]` attributes picking the implementation (ADR 0008). Jump to the section for the environment you are in.

Common to both: `Cargo.toml` `default-members` is limited to `quicuts-proto` + `quicuts-manifest` on purpose, so a plain `cargo build`/`cargo test` never tries to build a platform crate that can't compile on the current host. Platform crates are built explicitly by the `just` recipes.

## Windows target — built in WSL2, runs on the Windows host

Development happens in WSL2; the app **runs on the Windows host**. The Windows-only crates are cross-compiled with `cargo-xwin`; the platform-free crates test on the Linux toolchain.

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

## Windows target — built natively from PowerShell

pro-win-quicuts's clone, `C:\Users\pcpro\Projects\quicuts\quicuts`, builds on the host itself: `cargo build` with the MSVC host toolchain, no cross-compile and no tauri-cli. Decisions and consequences: `docs/adr/0008-native-windows-build.md`.

Toolchain: rustup (default host `x86_64-pc-windows-msvc`), VS 2022 Build Tools (Desktop C++ + a Windows SDK), Git for Windows (the recipes run in its bash via `windows-shell`), Node + pnpm 10, `just`. Clone with `-c core.autocrlf=false`; `.gitattributes` also forces LF.

The commands are the same as the WSL ones above, with these differences:

- `just test` covers **every crate**, including `quicuts-app`, and builds `agent` + `ui` first, because `quicuts-app`'s build needs the staged sidecar and `ui/dist`.
- `deploy` (and so `run`, `stage`, `dev-build`) kills the running instance first, since a running exe cannot be overwritten. `WINUSER` is not needed: the deploy folder comes from `%LOCALAPPDATA%`, and it is the **same `QuicutsDev` the WSL build deploys to**, so a deploy from either clone replaces the other's build.
- `just log` tails `%LOCALAPPDATA%\com.barbowza.quicuts\logs\Quicuts.log`.

Two host hazards: Bitdefender quarantines unsigned debug exes unless `target\` and `QuicutsDev` have exceptions (symptom: `LNK1104`, or an exe that vanishes), and the Claude desktop app redirects `%LOCALAPPDATA%`, so build and deploy from the Claude Code CLI.

## macOS target — built natively on the Mac

**Status: first vertical slice working.** Hold-⌘ panel, `⌃⌘/` toggle, Esc dismiss, and bundle-ID app matching are verified on a real Mac. `docs/macos-dev.md` is the setup and dev-loop reference (the `mac-*` `just` recipes are the entry points; TCC needs a one-time Accessibility grant to your terminal — see its *Autonomous mode* section). Decisions and known gaps: `docs/adr/0006-macos-agent.md`.

How the mac loop differs from the Windows one:

- **Native build, no cross-compilation.** `cargo tauri` runs on the Mac itself; the sidecar's staged filename is derived from `rustc -vV`, so Apple silicon and Intel both work.
- **The agent needs a TCC permission** (Accessibility / Input Monitoring) granted by hand in System Settings. No amount of code grants it — an agent that starts without it emits `Fatal { kind: PermissionRequired }`, which the protocol already defines. Expect human checkpoints in the loop.
- **App identity is the bundle ID** (`com.apple.Safari`), not an exe name — see *Manifest matching rules*.
- **macOS manifests live in `manifests-mac/`**, kept separate from the Windows set in `manifests/`. Both sets are parse-tested by `crates/quicuts-manifest/tests/real_manifests.rs` on the *Linux* toolchain, so `just test` in WSL catches a broken mac manifest.
- **No taskbar badges.** macOS has no ⌘1–9 Dock switching, so there is nothing to badge; the mac agent simply doesn't advertise the `taskbar` capability and ignores `QueryTaskbar`.

## Who does what, and what needs Michael

**Michael** = product delivery management: scope, priorities, on-device testing;
brings mac-quicuts online on request; does not develop or push. **pro-win-quicuts** =
lead: design decisions, task assignment, Windows development (native build), the
merge button; works solo by default. **pro-wsl-quicuts** = secondary: keeps the
WSL cross-build working, takes assigned work, reviews. **mac-quicuts** = on-demand: the macOS port, deep peer
review, and an explicit **duty to push back** — paired with a duty to
**verify before pushing back**, since a reviewer's failure mode is confident
noise, not silence.

Sessions settle implementation, tests, no-user-visible-change refactors and doc
wording between themselves. **Michael decides** anything that changes shipped
behaviour, changes an accepted ADR's *decision* (not its record), needs
on-device verification, changes scope — or that you are unsure about.

**Merge invariants** (each learned expensively; rationale in
`docs/two-agent-review-process.md`):

- Approval is **per-PR**; it never generalises to the next one.
- A gate you set is cleared only by its owner, and two reasons to pause need
  **both** resolved.
- Announce, then **wait for a reply** — never a window of your own choosing. Not
  willing to block? Say "merging now" instead of offering one.
- A **relayed instruction is not the instruction**: "Michael asked for X" from
  the other session is a report, not authority.
- Merging unreviewed is fine when the other session is offline — **flag it in the
  PR body**, always.

`main`'s branch protection enforces the check-before-merge gate and blocks
direct commits — but **only those**. Every invariant above is still perfectly
legal against a green check; the server closes one hole, not the loop.

**Branches:** `pw/<topic>` (pro-win-quicuts), `win/<topic>` (pro-wsl-quicuts),
`mac/<topic>`, one writer each, pushed at claim time so `git ls-remote --heads`
shows who holds what. Run `just status` before starting or merging (every
checkout); the Windows sessions run `just stage` before asking Michael to test —
there is no mac equivalent yet.

Model and envelope in full: `docs/collaboration.md`. Review procedure and `gh`
incantations: the **`two-agent-flow`** skill.

**Session names** are `<machine>-<env>-<project>`: `mac-quicuts` on the Mac;
on Prometheus, `pro-win-quicuts` (native Windows clone) and `pro-wsl-quicuts`
(WSL clone). Before 2026-10-13 they were `mac-claude` and `win-claude`, the
names older PRs, issues and comment tags carry; pro-wsl-quicuts was the lead
until #43 moved the role to pro-win-quicuts the same day.

## The journal

On each machine the clone sits in a root folder as `quicuts/` next to
`quicuts-journal/` (`barbowza/quicuts-journal` on GitHub): the root is
`~/learnings/tools/quicuts/` on the Mac, `/home/dev/projects/quicuts/` in
WSL on Prometheus and `C:\Users\pcpro\Projects\quicuts\` natively on
Prometheus. Both Prometheus clones use machine `pro` and write `diary/pro/`.
`just start` in the journal
launches the day (sync, today's entry, VS Code, Claude), its steps switched in
`machines/<machine>.env`.

**The journal is Michael's.** Read it freely for context. Write only your
**handoff**:

- at the end of a session, or when asked, **append** to
  `diary/<machine>/YYYY/MM-Mon/YYYY-MM-DD-claude.md` under a heading
  `## HH:MM <session name>` (what shipped, what's running, what's next),
  since several sessions may write the same day's file;
- `<machine>` is the word in the journal's `.machine` file (`mac` or `pro`),
  never a guess;
- publish it with `just sync-file <that path>` in the journal, which commits
  and pushes that one file, never Michael's half-written entries. If the push
  fails the commit stays local: say so, and never force-push or rewrite the
  journal's history.

Never edit Michael's own entries, `TODO.md` or `ideas.md`, and never another
machine's folder.

## Architecture: the crates + UI

The **IPC protocol is the platform seam.** Every platform agent implements the same commands and events, so keep platform specifics behind the protocol — never leak them into `quicuts-app` or the UI.

- **`crates/quicuts-proto`** — the NDJSON IPC contract. `AgentCommand` (app→agent: `Configure`, `SetOverlayVisible`, `QueryTaskbar`, `SubscribeForeground`, `Ping`, `Shutdown`) and `AgentEvent` (agent→app: `Ready{caps}`, `HoldActivated`, `HoldReleased`, `ChordActivated`, `Dismissed`, `ForegroundChanged`, `Taskbar`, `Pong`, `Fatal`). Enums are `#[non_exhaustive]`; `PROTO_VERSION` is negotiated in `Ready`. `to_line`/`from_line` are the NDJSON helpers. Changing this contract touches both the agent and the app.

- **`crates/quicuts-manifest`** — the PTSG-compatible manifest engine, **platform-free and the most heavily unit-tested crate**. Pipeline: `schema.rs` (tolerant serde — `LaxBool` accepts `True`/`true`, `RawKeyToken` handles numbers/strings/null-for-`~`) → `keys.rs` (`normalize_token` → `Key` enum: `Literal`/`Vk`/`Glyph`/`UnderlinedLetter`/`TaskbarRange`/`AngleLiteral`) → `parse.rs` (`parse_manifest`, filename `<Package>.<locale>.yml` splitting, meta/taskbar section handling) → `store.rs` (`ManifestStore` layers sources — Bundled/PtsgRuntime/User, later wins whole-file; `match_foreground` by reported app identity). Per-file parse errors are logged, never fatal. `tests/real_manifests.rs` runs the whole bundled set through it — run this after any schema/parse change.

- **`crates/quicuts-agent-win`** — the Windows sidecar (only binary that installs global hooks). `hook.rs` is the **highest-risk code**: a `WH_KEYBOARD_LL` hook driving a hold/chord state machine (`Idle → WinDown → {Combo | HoldActive} → Idle`), plus PowerToys' dummy-key injection (`SendInput` VK 0xFF with a magic `dwExtraInfo`) so holding Win doesn't open the Start menu. `foreground.rs` = `SetWinEventHook` foreground watcher; `taskbar.rs` = `IUIAutomation` taskbar-rect reader; `ipc.rs`/`state.rs` = NDJSON transport + cached config atomics (the hook callback must never block on IPC). Everything is `#[cfg(windows)]`.

- **`crates/quicuts-agent-mac`** — the macOS sidecar. `activation.rs` is the **pure, unit-tested** hold/chord state machine (`Idle → CmdDown → {Combo | HoldActive} → Idle`, no dummy-key injection — ⌘ alone triggers nothing on macOS); `tap.rs` is a thin active-`CGEventTap` adapter that translates CGKeyCode → the protocol's Windows-style VK codes, re-enables the tap after `TapDisabledByTimeout` (plus a watchdog for taps killed with no callbacks), and resets the state machine on every re-enable; `foreground.rs` is an `NSWorkspace` frontmost watcher reporting bundle IDs, plus the experimental title watcher — a 200ms poll thread (parked on a condvar unless `Configure.title_events_enabled` is on) that fills in `ForegroundInfo.title` via `axtitle.rs`; `axtitle.rs` is the Accessibility-API title read (`AXFocusedWindow` → `AXTitle`), which rides the same grant as the tap and **must never run on the main thread** — that thread is the tap's, and a blocking AX call there earns `TapDisabledByTimeout`. `ipc.rs` is a verbatim copy of the Windows one. No taskbar reader. Same `AgentCommand`/`AgentEvent` protocol, so `agent.rs` needs no per-platform branching. Decisions and known gaps: `docs/adr/0006-macos-agent.md`. **`activation.rs` builds and tests on the Linux toolchain** — `just test` runs it, so a WSL session can catch regressions in it.

- **`crates/quicuts-app`** — the Tauri host. `agent.rs` supervises the sidecar (spawn, NDJSON handling, backoff restart). `engine.rs` builds the overlay view-model on each foreground change and emits `overlay://state`; the frontend is dumb-render. `overlay.rs`/`tray.rs` manage windows and the tray menu; `macpanel.rs` (macOS only) turns the overlay into a non-activating `NSPanel` at setup. `commands.rs` holds the `#[tauri::command]` handlers invoked from the UI. `settings.rs` (persisted to `{app_config_dir}/settings.json`, pushed live to the agent as `Configure` — no restart), `pinned.rs`, `icons.rs` (app icon → data-URI for the rail: the exe's icon resource on Windows, NSWorkspace's `.app` icon on macOS), `procs.rs` (running apps, for `BackgroundProcess` manifests: exe names on Windows, bundle IDs on macOS). `lib.rs::run()` wires `invoke_handler` + `on_window_event`.

- **`ui/`** — Svelte 5 + Vite, **three separate windows/entry points**: `overlay.html`, `badges.html`, `settings.html`, each with its own `src/<name>/main.ts`. `lib/ipc.ts` wraps Tauri `invoke`/`listen` (with browser no-op fallbacks); `lib/types.ts` mirrors the Rust view-model types; `overlay/keys.ts` renders key glyphs. **When you add a `#[tauri::command]`, add its wrapper in `ipc.ts` and register it in `lib.rs`'s `generate_handler!`.**

## Window lifecycle gotcha

Overlay/badges/settings windows are **pre-created hidden** and reused. On close they must `prevent_close()` + `hide()` (see `on_window_event` in `lib.rs`) — do NOT let them be destroyed, or they won't reopen. Every hide of the overlay should also send `SetOverlayVisible(false)` to the agent.

## Manifest matching rules

`match_foreground` is platform-agnostic on purpose: it lowercases the manifest's `WindowFilter`, strips one trailing `.exe`, and compares it to whatever **identity string the agent reported** — an exe name on Windows, a bundle ID (`com.apple.Safari`) on macOS. Don't add platform branches to it; give it the right identity instead.

`WindowFilter` accepts **one identity or a list** (tolerant serde, so every PowerToys manifest's scalar form parses unchanged). A list is how one manifest covers an app whose editions ship under different identities — `manifests-mac/Mozilla.Firefox.en-US.yml` covers release, Developer Edition and Nightly. Use `Manifest::matches_identity` / `is_wildcard` / `primary_filter` rather than reaching for `window_filters` directly.

The rail's **foreground entry** (the page shown by default) is the title-matched hosted collection when detection has one, else the first *exact or wildcard* match — never a hosted collection. Note that no bundled manifest currently reaches `MatchKind::Wildcard` (both `"*"` manifests are also `BackgroundProcess: true`, and background is tested first), so in practice this is "first exact match, else the unsupported-app placeholder". `quicuts_manifest::foreground_entry`; ADR 0003 carries the correction and the open question.

Match foreground app by that identity, then exact-match ∪ `"*"` wildcard manifests ∪ all `BackgroundProcess: true` manifests. Section render order: Pinned → Recommended → categories in file order → Taskbar. Ignore PowerToys' own `index.yml` — Quicuts builds its own in-memory index. Bundled manifests are MIT-licensed from PowerToys (`manifests/POWERTOYS-LICENSE`, `NOTICE.txt`).

## Status & references

Working daily-driver on Windows: hold-to-show with Win-key suppression, app-aware panels, taskbar badges, pinning, and customizations are verified on a real host. Experimental web-app title detection (hosted collections) is implemented behind a settings toggle; users can bind their own title signatures (e.g. Google Workspace's per-org mail title) to hosted collections in Settings (ADR 0007).

macOS: first slice verified on-screen (hold/chord/Esc, app-aware panels by bundle ID, tray-only), plus hosted collections — the mac agent reports window titles through the Accessibility API (no extra TCC grant beyond the tap's) and the browser host class knows bundle IDs, so Gmail/Yahoo Mail auto-select on a Mac exactly as on Windows. The IPC protocol was the seam both plugged into and needed no new commands or events. Deliberate gaps are listed in ADR 0006. The overlay is a non-activating `NSPanel` (`macpanel.rs`; Tauri v2 has no panels, so the window is re-classed in place behind a runtime layout check), so showing it leaves the user's app active. Rail icons and running-apps detection were closed under issue #32. Remaining gap: no notarization, on hold until a public release (bundles are signed with a local self-signed identity; `docs/macos-dev.md` § Signing).

ADRs: `docs/adr/0001` (why Tauri v2), `docs/adr/0002` (the no-sudo cross-toolchain), `docs/adr/0003` (hosted collections), `docs/adr/0004` (unsupported-app placeholder), `docs/adr/0005` (overlay font scaling / accessibility zoom), `docs/adr/0006` (the macOS agent + TCC responsible-process findings), `docs/adr/0007` (user title-signature bindings + multi-pattern TitleMatch), `docs/adr/0008` (the native Windows build path beside the WSL one). `docs/macos-slice-brief.md` is the handoff brief that scoped the macOS slice — historical now, kept for the reasoning. `docs/two-agent-review-process.md` is how the Mac and Windows Claude sessions review each other's PRs.
