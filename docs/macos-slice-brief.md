# Quicuts — macOS agent, first vertical slice

> **This is a handoff brief, not a spec of what exists.** It was written on the Windows side
> after a design session and committed to the `mac-version` branch so the macOS work can start
> from a settled set of decisions. Everything under "Architecture decisions" is already agreed;
> everything under "Out of scope" is deliberately deferred.

You are working on **Quicuts**, a cross-platform clone of PowerToys *Shortcut Guide v2*: a
side-docked, app-aware panel of keyboard shortcuts shown while the user holds a modifier or
presses a chord. It's a Tauri v2 app — Rust backend, Svelte 5 frontend. It works today as a
daily driver on Windows. **There is no macOS support yet. You are building the first slice of it.**

Start by reading `CLAUDE.md` at the repo root — it describes the architecture you're extending
and already covers both development environments. Then read this whole brief before doing
anything.

Work on the branch **`mac-version`**. Do not merge to `main`.

---

## Mission: a thin vertical slice

**Done means:** on this Mac, holding ⌘ for 900ms makes the Quicuts panel slide in from the
screen edge, showing shortcuts for the app that was frontmost, with that app's name in the
rail. Releasing ⌘ hides it. `⌃⌘/` toggles it. Esc dismisses it.

That is the whole goal. It proves the IPC protocol is a real platform seam. Everything else
is explicitly deferred — see **Out of scope** at the bottom, and do not drift into it.

---

## Non-negotiable invariants (from CLAUDE.md — these survive the port)

1. **All system-input access lives in a sidecar process, never the main app binary.** On
   Windows that's `quicuts-agent-win.exe`, shipped as a Tauri `externalBin`. You are building
   `quicuts-agent-mac` the same way. The event tap does **not** go in `quicuts-app`.
   (The Windows rationale is AV/keylogger heuristics; on macOS the payoff is crash isolation
   plus keeping the app's supervision code identical across platforms.)

2. **Privacy invariant: no IPC message ever carries the identity of a key the user pressed.**
   The only key data on the wire is the activation-chord *configuration* flowing app→agent
   (`ChordSpec`). The agent reports semantic transitions only — `hold_activated`,
   `chord_activated`, `dismissed`, `foreground_changed`. Never keystrokes. Preserve this.

---

## Architecture decisions — already made, do not re-litigate

These were settled in a design session. Treat them as given. If you find hard evidence that
one of them is *impossible* (not merely inconvenient), stop and say so rather than working
around it silently.

| Area | Decision |
|---|---|
| **App identity** | The agent reports the **bundle identifier** in `ForegroundInfo.exe_name` (e.g. `com.apple.Safari`) and the `.app` bundle path in `exe_path`. Bundle IDs are macOS's stable identity. No proto change needed — see the note on `match_foreground` below. |
| **Hold trigger** | Either ⌘ (left or right), 900ms, mirroring hold-Win. |
| **Chord default** | `⌃⌘/` on macOS. Windows keeps `Win+Shift+/`. Note that `⌘⇧/` — the literal Windows equivalent — is the system Help-menu shortcut on macOS and must not be taken. |
| **Settings chord** | `⌘,` on macOS (universal prefs shortcut). Handled inside the app's own focused windows, never by the tap. |
| **Sidecar** | Keep it. `crates/quicuts-agent-mac`, shipped as a Tauri `externalBin`. |
| **Overlay window** | Use the existing Tauri window config as-is and **accept that showing the panel activates the app** (menu bar switches to Quicuts). The macOS-correct fix is a non-activating `NSPanel`, which Tauri v2 doesn't expose — that is a deliberate, documented gap for a later milestone, not your problem now. |
| **Dock icon** | Set `ActivationPolicy::Accessory` so Quicuts is tray-only: no Dock icon, no ⌘Tab entry. |
| **Manifests** | New sibling directory `manifests-mac/` at the repo root. Do **not** touch the existing 36 files in `manifests/`. |
| **State machine** | The hold/chord logic goes in a **pure, platform-free, unit-tested module**. The CGEventTap is a dumb adapter that feeds it. Details below. |
| **Docs** | Write `docs/adr/0006-macos-agent.md` and `docs/macos-dev.md`. `CLAUDE.md` has already been updated on this branch to cover both dev environments — do **not** edit it or `README.md` further; put any suggested edits in the PR description instead. |
| **Protocol** | `PROTO_VERSION` stays `1`. No new commands or events. |

---

## Facts already established — don't spend time rediscovering these

The Windows-side codebase was read before this brief was written. These are verified:

- **`ManifestStore::match_foreground` (`crates/quicuts-manifest/src/store.rs:166`) is already
  platform-agnostic.** It lowercases the manifest's `WindowFilter`, strips one trailing
  `.exe`, and string-compares it to whatever identity the agent reported. Feed it a bundle ID
  and put bundle IDs in `manifests-mac/*.yml` and it just works. **Do not modify this function.**

- **`crates/quicuts-app/src/agent.rs:164` ignores the `caps` list entirely** — it only logs
  `proto_version`. So an agent that never advertises `taskbar` degrades gracefully with zero
  app-side changes; `QueryTaskbar` will still be sent and your agent should simply ignore it.

- **`crates/quicuts-app/src/engine.rs:60`** already sets
  `PLATFORM = if cfg!(target_os = "macos") { "macos" } else { "windows" }` and ships it in the
  overlay view model.

- **`ui/src/overlay/keys.ts` already renders macOS keycaps.** `Platform = "windows" | "macos"`,
  and `winModCap` (line ~202) emits ⌘ ⌃ ⌥ ⇧ when platform is `"macos"`. **The frontend needs
  no changes for the slice.**

- **`ui/src/settings/Settings.svelte:93` captures chords via the DOM's legacy `e.keyCode`,**
  which yields Windows-style VK values on macOS WebKit too (`/` is 191 on both). So
  `ChordSpec` stays platform-free and **your agent translates VK → CGKeyCode internally.**
  Don't change the wire format.

- **`crates/quicuts-agent-win/src/ipc.rs` is 100% platform-free** (NDJSON stdin reader thread +
  stdout writer thread). Copy it into the mac agent essentially verbatim.

- **`crates/quicuts-app/src/lib.rs:186`, `bundled_manifests_dir`**, resolves the manifest
  directory through a candidate-path list. Add `manifests-mac` candidates ahead of the
  existing ones under `cfg(target_os = "macos")`.

- **`crates/quicuts-app/src/appname.rs:87`** is a `#[cfg(not(windows))]` stub returning `None`,
  so the rail would show the raw identity string. With bundle-ID identity that means the rail
  literally reads `com.apple.Safari`. You are fixing this one (see Step 3).

- **`icons.rs` and `procs.rs` have the same `cfg(not(windows))` stubs.** Leave them alone —
  the rail falls back to a glyph, which is fine for the slice.

- **`quicuts-proto` already anticipates you:** `FatalKind::PermissionRequired` exists with the
  doc comment *"macOS: Accessibility / Input Monitoring permission missing."* Emit it.

- **The Windows hold/chord state machine** lives in `crates/quicuts-agent-win/src/hook.rs`
  (`Idle → WinDown → {Combo | HoldActive} → Idle`). Read it for the semantics — especially how
  a second key press drops it into `Combo` so `Win+Tab` never triggers a hold. **It has zero
  tests.** You are not fixing that; you are simply not repeating it.

- **PowerToys' dummy-key injection** (`SendInput` VK 0xFF with a magic `dwExtraInfo`) exists
  only to stop the Start menu opening on Win release. **macOS has no equivalent problem — do
  not port it.** Your state machine is strictly simpler.

- **`crates/quicuts-app/conf/dev-remote.json`** is the precedent for per-build Tauri config
  overlays merged with `--config`. Use that pattern for mac bundle settings rather than
  editing `tauri.conf.json` (whose `"targets": ["nsis"]` is Windows-only).

- **Workspace `Cargo.toml` `default-members` is deliberately limited** to `quicuts-proto` +
  `quicuts-manifest` so a Linux dev box can `cargo test` without building platform crates.
  Add `quicuts-agent-mac` to `members` but **not** to `default-members`.

---

## Step 0 — toolchain bootstrap  ⚠️ HUMAN CHECKPOINT

This Mac is Apple Silicon (`aarch64-apple-darwin`). Node and pnpm are installed. **Xcode.app
is installed but the command-line tools are not wired up, and Rust is not installed at all.**

Some of this needs `sudo`, which you cannot supply. Print the exact commands and **stop and
ask the user to run them**, then verify:

```bash
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
sudo xcodebuild -license accept
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo install just
cargo install tauri-cli --version "^2"
```

Verify with `cc --version`, `cargo --version`, `just --version`, `cargo tauri --version`, and
`rustc -vV | grep host` (expect `aarch64-apple-darwin`). Do not proceed until all pass.

---

## Step 1 — read before you build

Read, in this order: `CLAUDE.md`, `crates/quicuts-proto/src/lib.rs` (the whole contract),
`crates/quicuts-agent-win/src/main.rs`, `ipc.rs`, `state.rs`, `hook.rs`,
`crates/quicuts-app/src/agent.rs`, `overlay.rs`, `lib.rs`, and one bundled manifest
(`manifests/+WindowsNT.Notepad.en-US.yml`) so you know the YAML shape.

Confirm `cargo test` passes on the platform-free crates before you change anything.

---

## Step 2 — research gate  ⚠️ HUMAN CHECKPOINT

Before writing the event tap, research and report your findings. The architecture above is
settled; **this gate is only about macOS specifics that could not be verified from the
Windows/WSL side.** Use current documentation (this repo's convention is the `ctx7` CLI for
library docs — `npx ctx7@latest library "Tauri" "<question>"` then `docs <id> "<q>"`).
Answer at least:

1. **`CGEventTap` vs `NSEvent.addGlobalMonitorForEvents`** — which do we need? We must be able
   to *swallow* events (the chord key, Esc while the panel is up), which rules out a passive
   monitor. Confirm the tap location/placement needed, and what happens when the system
   disables a tap for being too slow (`kCGEventTapDisabledByTimeout`) — the Windows agent has
   no equivalent, so recovery is new logic.
2. **Accessibility vs Input Monitoring** — which TCC permission does that tap actually require
   on current macOS, and how do we query grant status (`AXIsProcessTrustedWithOptions`?) so we
   can emit `FatalKind::PermissionRequired` instead of silently failing?
3. **Which process does TCC prompt for** when a Tauri app spawns an `externalBin` sidecar —
   the sidecar, or the responsible parent app bundle? This is the single biggest open risk to
   the sidecar architecture. Determine it, empirically if the docs are ambiguous.
4. **How ad-hoc signing affects the grant across rebuilds** — does the user have to re-approve
   in System Settings every time you rebuild? If so, what's the mitigation (stable ad-hoc
   identity / a self-signed cert / granting the terminal)? This feeds the "autonomous mode"
   doc you'll write in Step 5.
5. **Tauri v2 macOS specifics** — `externalBin` naming for `aarch64-apple-darwin`, how to set
   `ActivationPolicy::Accessory`, and the right `bundle.targets` for a mac config overlay.
6. **Which Rust crate** you'll use for the AppKit/CoreGraphics calls (`objc2` + `objc2-app-kit`
   + `core-graphics`, or similar) and why.

Present these findings and your implementation plan, then **wait for the user's approval**
before writing the tap.

---

## Step 3 — build

### 3a. `crates/quicuts-agent-mac`

Mirror the layout of `quicuts-agent-win`: `main.rs`, `ipc.rs` (copy verbatim), `state.rs`
(cached config atomics — the tap callback must never block on IPC), plus:

- **`activation.rs` — the pure state machine.** No AppKit, no CoreGraphics, no I/O. It takes
  key events as plain data (`vk: u32, down: bool, timestamp: Instant`-or-monotonic-ms) plus
  the current config, and returns actions (`Emit(HoldActivated)`, `Swallow`, `PassThrough`,
  `ArmTimer(ms)`, …). Port the semantics from `hook.rs`'s `Idle → WinDown → {Combo | HoldActive}
  → Idle`, minus the dummy-key injection. **Unit test it**, at minimum:
  - holding ⌘ past the threshold fires `hold_activated`
  - ⌘Tab (a second key arrives) drops to `Combo` and never fires a hold
  - releasing ⌘ before the threshold fires nothing
  - the chord fires `chord_activated` and swallows both the down and the up
  - pressing the chord again while visible emits `Dismissed { reason: ChordAgain }`
  - Esc while the overlay is visible emits `Dismissed { reason: Esc }` and is swallowed
  - Esc while *not* visible is passed through untouched
  - an excluded bundle ID in the foreground prevents hold from arming
- **`tap.rs`** — the CGEventTap adapter. Translate CGKeyCode → the Windows-style VK the
  protocol speaks, feed `activation.rs`, act on what it returns. Handle
  `kCGEventTapDisabledByTimeout` by re-enabling. Recover or emit `Fatal` on permission loss.
- **`foreground.rs`** — watch frontmost-app changes (`NSWorkspace`
  `didActivateApplicationNotification`) and emit `ForegroundChanged` with the **bundle ID** in
  `exe_name` and the `.app` path in `exe_path`. Also resolve the frontmost app on demand, since
  `HoldActivated`/`ChordActivated` carry the app that was frontmost *before* activation.
- **`main.rs`** — `#[cfg(target_os = "macos")]` gating throughout, with a
  `#[cfg(not(target_os = "macos"))]` `main` that errors out, exactly like the Windows agent.
  Advertise `caps`: `hold`, `chord`, `foreground`. **Not** `taskbar`, **not** `title`.
  Ignore `QueryTaskbar` silently.

Binary name: `quicuts-agent` (same as Windows) so `app.shell().sidecar("quicuts-agent")` in
`agent.rs:23` needs no change.

### 3b. `quicuts-proto`

Only change: make `ChordSpec::default()` and `ChordSpec::settings_default()` platform-conditional
with `cfg!(target_os = "macos")` — `⌃⌘/` is `{ win: true, ctrl: true, shift: false, alt: false,
vk: 0xBF }`, and `⌘,` is `{ win: true, ctrl: false, shift: false, alt: false, vk: 0xBC }`.
Keep the existing Windows values on the other branch. Update the doc comments. Keep the
existing tests green and add one asserting the mac values under a mac cfg.

### 3c. `quicuts-app`

Make it compile and behave on macOS:
- `Cargo.toml`: add a `[target.'cfg(target_os = "macos")'.dependencies]` section for whatever
  Info.plist reading needs.
- `appname.rs`: implement `file_description` for macOS — read `Contents/Info.plist` from the
  `.app` path and prefer `CFBundleDisplayName`, falling back to `CFBundleName`, then to the
  existing stem behavior. This is a plain file read: no TCC, no AppKit. Without it the rail
  shows `com.apple.Safari`.
- `lib.rs`: add `manifests-mac` to the `bundled_manifests_dir` candidates under
  `cfg(target_os = "macos")`; set `ActivationPolicy::Accessory` during setup.
- Anything else that fails to compile — fix minimally and mechanically. **Do not restructure
  Windows code paths.**
- `crates/quicuts-app/conf/macos.json`: a config overlay for mac bundle targets, merged via
  `--config`, following the `dev-remote.json` precedent.

### 3d. `manifests-mac/`

Author four manifests in the existing PTSG YAML shape (copy the structure from a bundled file;
`WindowFilter` holds a bundle ID):
- a Shell/System manifest — `WindowFilter: "*"`, `BackgroundProcess: true`, containing genuine
  system-wide macOS shortcuts (Spotlight, Mission Control, screenshot, ⌘Tab)
- `com.apple.finder`
- `com.apple.Safari`
- `com.microsoft.VSCode`

Keep them small — a handful of real shortcuts each. They exist to prove matching works, not to
be complete.

### 3e. `justfile`

**Append** `mac-*` recipes at the end; don't restructure the existing WSL ones.
- `mac-test` → `cargo test -p quicuts-proto -p quicuts-manifest -p quicuts-agent-mac`
- `mac-agent` → build the sidecar and stage it into `crates/quicuts-app/binaries/` under the
  `aarch64-apple-darwin` name Tauri expects
- `mac-ui`, `mac-build`, `mac-run`
- **`mac-run` must echo a reminder line** every time, pointing at the autonomous-mode section:
  `@echo "TIP: permission checkpoints can be removed — see docs/macos-dev.md § Autonomous mode"`

Also add `crates/quicuts-agent-mac` to workspace `members` — **not** `default-members`.

---

## Step 4 — run and verify  ⚠️ HUMAN CHECKPOINTS

You cannot grant TCC permissions; only the user can, by clicking in System Settings. When you
reach the first run:

1. Build and launch. **Stop and tell the user exactly which pane to open** (System Settings →
   Privacy & Security → Accessibility, and/or Input Monitoring) and which entry to enable.
   Wait for them to confirm.
2. **Stop and ask the user to verify on screen**, one at a time:
   - holding ⌘ for ~900ms shows the panel; releasing hides it
   - ⌃⌘/ toggles it; Esc dismisses it
   - the rail shows a readable app name (e.g. "Safari", not `com.apple.Safari`)
   - switching apps and re-triggering shows the *new* app's shortcuts
   - ⌘Tab does **not** trigger the panel
   - no Dock icon, no ⌘Tab entry for Quicuts

**Never report a runtime behavior as working unless the user confirmed they saw it.** Say
plainly what is unverified.

If a rebuild invalidates the TCC grant and forces re-approval, record exactly what happened —
that observation is the whole basis for the autonomous-mode doc.

---

## Step 5 — document, then PR

**`docs/adr/0006-macos-agent.md`** — follow the shape of the existing ADRs in `docs/adr/`.
Cover: the decisions in the table above and *why*; what you found in the Step 2 research
(especially the TCC responsible-process answer, which is the load-bearing fact for keeping
the sidecar architecture); and an explicit **Known gaps** section listing at minimum —
non-activating NSPanel, app icons, running-apps detection, Dock badges (no macOS equivalent
exists — say so), hosted collections (`BUILTIN_BROWSER_EXES` in
`crates/quicuts-manifest/src/host.rs` holds `chrome.exe`-style names and needs bundle IDs),
title detection, code signing and notarization, and launch-at-login.

**`docs/macos-dev.md`** — how to set up and run on macOS: toolchain bootstrap, the `mac-*` just
recipes, where TCC permissions are granted, how to read logs, known gotchas. It must contain a
section titled exactly **"Autonomous mode"** explaining how the user can later remove the human
checkpoints from this workflow: what to pre-grant, whether a stable signing identity avoids
re-approval on rebuild, and what a future Claude session can then safely do unattended. Be
concrete and base it on what you actually observed, not on what you assume.

**Do not edit `CLAUDE.md` or `README.md`** — CLAUDE.md was already made cross-platform on this
branch. Instead, end the PR description with a section "Suggested CLAUDE.md / README edits"
containing the exact wording you'd add.

Then commit on `mac-version`, push, and open a PR against `main`. The PR description should
state clearly what was verified on screen by the user versus what merely compiles.

---

## Out of scope — do not build these

Non-activating NSPanel · app icons in the rail · running-apps / `BackgroundProcess` detection ·
Dock badges (macOS has no ⌘1–9 Dock switching; there is nothing to badge) · title detection /
hosted collections · code signing, notarization, DMG · launch-at-login · porting the remaining
32 manifests · refactoring the working Windows `hook.rs` · any change to
`ManifestStore::match_foreground` · any new command or event in the protocol.

If you finish early, **stop and report** rather than starting one of these.
