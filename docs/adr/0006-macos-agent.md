# ADR 0006 — macOS agent (first vertical slice)

Status: accepted (2026-07-29)

## Context

Quicuts is a working daily driver on Windows; the IPC protocol
(`quicuts-proto`) was designed as the platform seam, but until this slice
nothing had ever plugged into the other side of it. The goal of the slice
was deliberately thin — hold ⌘ 900ms → panel with the frontmost app's
shortcuts, `⌃⌘/` toggle, Esc dismiss, readable app name in the rail — to
prove the seam is real before investing in macOS breadth. The scope and
the decisions below were settled in a design session and recorded in
`docs/macos-slice-brief.md` before any macOS code was written.

## Decision

The mac agent is `crates/quicuts-agent-mac`, a sidecar binary named
`quicuts-agent` (same as Windows, so `agent.rs`'s
`sidecar("quicuts-agent")` call is untouched), and the app-side supervisor,
engine, and frontend needed **zero structural changes** — the slice
validated the seam.

| Area | Decision | Why |
|---|---|---|
| App identity | Agent reports the **bundle identifier** in `ForegroundInfo.exe_name`, the `.app` path in `exe_path`. | Bundle ids are macOS's stable app identity. `ManifestStore::match_foreground` normalizes both sides (lowercase, strip one `.exe`), so bundle ids flow through the existing matcher unchanged — no platform branch was added. |
| Sidecar kept | The event tap lives in `quicuts-agent-mac`, shipped as a Tauri `externalBin`, never in `quicuts-app`. | Crash isolation plus one shared supervision path in `agent.rs`; also the TCC story stays coherent (below). |
| Event capture | **Active CGEventTap** — `kCGSessionEventTap`, `kCGHeadInsertEventTap`, `kCGEventTapOptionDefault`, mask keyDown/keyUp/flagsChanged. | Passive `NSEvent` global monitors cannot consume events, and the slice must swallow the chord key and Esc-while-visible. A HID-level tap needs root; the session tap doesn't. |
| Tap recovery | `TapDisabledByTimeout`/`ByUserInput` arrive as event types in the callback: re-enable there (Hammerspoon pattern), plus a 5s watchdog polling `CGEventTapIsEnabled` for taps that die with no callbacks (sleep/wake). | The Windows agent has no equivalent; a stalled callback or a sleep cycle silently kills a tap otherwise. |
| State machine | Pure, platform-free `activation.rs` (`Idle → CmdDown → {Combo | HoldActive} → Idle`), unit-tested (11 tests); `tap.rs` is a dumb adapter that translates CGKeyCode → the protocol's Windows-style VK codes and acts on returned decisions. | The Windows `hook.rs` machine has zero tests; the port fixes that instead of repeating it. The dummy-key injection was **not** ported — ⌘ alone triggers nothing on macOS, so the machine is strictly simpler. |
| Chord defaults | `⌃⌘/` toggles the overlay; `⌘,` opens settings. `ChordSpec::default()`/`settings_default()` are `cfg!(target_os = "macos")`-conditional; the wire format is unchanged (the settings UI captures Windows-style keycodes on both platforms; agents translate internally). | `⌘⇧/` — the literal Windows equivalent — is the system Help-menu shortcut and must not be taken. |
| Hold trigger | Either ⌘ (left or right), 900ms, mirroring hold-Win. | |
| Overlay window | Existing Tauri window config as-is; showing the panel activates the app (menu bar switches to Quicuts). | The macOS-correct fix is a non-activating `NSPanel`, which Tauri v2 doesn't expose. Deliberate, documented gap. |
| Dock | `ActivationPolicy::Accessory` set in `Builder::setup`: tray-only, no Dock icon, no ⌘Tab entry. | |
| Manifests | `manifests-mac/` sibling directory (4 files: system-wide `"*"` + Finder, Safari, VS Code by bundle id); `bundled_manifests_dir` prefers `manifests-mac` candidates under `cfg(target_os = "macos")`. Windows set untouched. | |
| Caps | Agent advertises `hold`, `chord`, `foreground` — not `taskbar` (no ⌘1–9 Dock switching exists to badge), not `title`. `QueryTaskbar` is ignored silently; `agent.rs` never read `caps` anyway, so degradation is free. | |
| Protocol | `PROTO_VERSION` stays 1. No new commands or events. The privacy invariant holds: the only key data on the wire is still the app→agent `ChordSpec`. | |
| Config overlay | `conf/macos.json` merged via `--config` (the `dev-remote.json` precedent): `bundle.targets: ["app"]`, `macOSPrivateApi: true` (required for the transparent overlay window on macOS), resources map for `manifests-mac/`. `tauri.conf.json` (`"targets": ["nsis"]`) untouched. | |
| Crates | servo `core-graphics` 0.25 (only Rust crate with a safe *consuming* tap API — `CallbackResult::Drop`) + `core-foundation` for the run loop; `objc2-app-kit`/`objc2-foundation`/`block2` for the `NSWorkspace` frontmost watcher; `AXIsProcessTrusted` via a direct ApplicationServices extern. | |

## The TCC facts (load-bearing for the sidecar architecture)

The open risk going in was: *which process does macOS TCC prompt for when a
Tauri app spawns an externalBin sidecar?* Answer (Apple DTS statements, Qt's
empirical work, and the Tauri issue tracker agree): **the responsible
process** — fork/exec children inherit their parent's TCC attribution.
Concretely:

- **Dev loop:** anything launched from a terminal — `cargo tauri dev`, the
  app, the sidecar under it — is attributed to the *terminal app*. Granting
  iTerm/Terminal Accessibility once covers every rebuild; grants never go
  stale because the terminal's signature never changes. This is what makes
  the sidecar architecture *cheaper* on macOS, not more expensive.
- **Bundled .app:** the app bundle is the responsible process; the grant
  appears under "Quicuts" and the sidecar's `AXIsProcessTrusted()` reads it
  — provided app + sidecar are coherently signed. Ad-hoc signatures have a
  CDHash-based designated requirement that changes every rebuild, which
  leaves TCC grants stale (checkbox ON, permission dead). Bundled dev
  builds need a stable identity (a free Apple Development certificate
  suffices, per Apple DTS guidance).
- An active keyboard tap needs **Accessibility** (not Input Monitoring —
  Accessibility subsumes it). Without the grant, `CGEventTapCreate`
  returns NULL; the agent checks `AXIsProcessTrusted()` first and emits
  `Fatal { kind: PermissionRequired }`, which the protocol had already
  reserved for exactly this. This path was exercised live: on an
  ungranted machine the agent emits the Fatal, `agent.rs` logs it,
  backs off through its 5 restarts, and flags the tray.
- There is no Info.plist purpose string for Accessibility/Input
  Monitoring; the user always flips the toggle in System Settings.

## Known gaps (deferred deliberately)

- **Non-activating panel** — showing the overlay activates Quicuts (menu
  bar switches). Needs `NSPanel` + `nonactivatingPanel`, not exposed by
  Tauri v2.
- **App icons in the rail** — `icons.rs` is a `cfg(not(windows))` stub; the
  rail falls back to a glyph. macOS impl would read the bundle's `.icns`.
- **Running-apps detection** — `procs.rs` stub: named `BackgroundProcess`
  manifests are never shown "because running". Needs
  `NSWorkspace.runningApplications`.
- **Dock badges** — permanently out: macOS has no ⌘1–9 Dock switching, so
  there is nothing to badge. The agent simply never advertises `taskbar`.
- **Hosted collections / title detection** — `BUILTIN_BROWSER_EXES` in
  `crates/quicuts-manifest/src/host.rs` holds `chrome.exe`-style names and
  needs bundle ids; window titles on macOS additionally require the
  Accessibility API per-window work the slice avoided. No `title` cap.
- **Code signing & notarization** — everything runs ad-hoc from the
  terminal today; a distributable .app needs a stable identity (see TCC
  facts above), hardened runtime, and notarization.
- **Launch-at-login** — `tauri-plugin-autostart` (LaunchAgent) is wired but
  unverified on macOS; the plist records the launching binary's path, so
  it's only meaningful for an installed .app.
- **Settings-window chord capture on macOS** — `Settings.svelte` captures
  `e.keyCode`, which matches Windows VKs for the keys tested, but the full
  macOS WebKit keyCode surface hasn't been audited.
