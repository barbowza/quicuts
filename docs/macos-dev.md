# Quicuts on macOS — setup & dev loop

Native build on the Mac (Apple Silicon, `aarch64-apple-darwin`). No
cross-compilation, no WSL. Architecture background: `docs/adr/0006-macos-agent.md`.

## One-time setup

Xcode command-line tools (needs sudo, once):

```bash
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
sudo xcodebuild -license accept
```

Rust + tools (no sudo):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
cargo install just
cargo install tauri-cli --version "^2"
```

Node + pnpm are assumed present. Verify:

```bash
cc --version && cargo --version && just --version && cargo tauri --version
rustc -vV | grep host   # expect aarch64-apple-darwin
```

## TCC permission (the one manual step)

The agent's active CGEventTap needs **Accessibility**. macOS attributes the
check to the *responsible process*, which for anything launched from a
terminal is the terminal itself:

> System Settings → Privacy & Security → **Accessibility** → enable your
> terminal app (iTerm, Terminal, …)

Grant it once and every `just mac-run` build works, forever — rebuilds
can't invalidate it because the grant belongs to the terminal, not to the
freshly-built binary. Input Monitoring is *not* required for the active
tap (Accessibility subsumes it).

Without the grant the agent exits with
`Fatal { kind: PermissionRequired }`; the app logs it (`just mac-log`),
retries 5 times, then flags the tray icon. After granting, relaunch
`just mac-run` — the given-up supervisor does not resume on its own.

A **bundled .app** (from `just mac-build`) is its own responsible process:
the grant lands under "Quicuts" in System Settings and — because dev
bundles are ad-hoc signed with a per-build CDHash — **goes stale on every
rebuild** (checkbox still ON, permission dead; re-toggle it off/on, or
`tccutil reset Accessibility com.barbowza.quicuts`). Use the terminal loop
for iteration; bundle builds are for permission-realistic testing only.
The local signing identity below fixes the staleness.

## Signing

`just mac-build` signs with hardened runtime, choosing the identity in this
order:

1. **`APPLE_SIGNING_IDENTITY`**, if set. With a **Developer ID
   Application** identity plus notarization credentials in the environment
   (`APPLE_ID`, `APPLE_PASSWORD` (an app-specific password), and
   `APPLE_TEAM_ID`; or `APPLE_API_KEY`, `APPLE_API_ISSUER` and
   `APPLE_API_KEY_PATH`), tauri signs **and notarizes**. That is the only
   path to a .app other Macs will open without a Gatekeeper override. It
   needs a paid Apple Developer account, which the project does not have
   yet.
2. **"Quicuts Local Signing"**, a self-signed identity, if
   `just mac-signing-identity` has been run on this Mac. Its designated
   requirement pins the bundle identifier and the certificate's hash,
   neither of which changes between rebuilds, so **the Accessibility grant
   survives rebuilds**. It is not distributable: Gatekeeper rejects it on
   any other Mac.
3. Otherwise **ad-hoc** (`cdhash`-only), with the per-rebuild staleness
   described above.

`just mac-signing-identity` is one-time and unattended. The identity lives
in its own keychain (`~/Library/Keychains/quicuts-signing.keychain-db`,
added to the user search list), not the login keychain. A login-keychain key
only works with codesign after `security set-key-partition-list`, which
needs your login password; without that, codesign fails with
`errSecInternalComponent`. The dedicated keychain's password is random and
kept in a 0600 file beside it, so `mac-build` can unlock it after a logout.
The script header has the removal commands.

**Launch at login** (Settings) registers a LaunchAgent pointing at the
*running* binary, so turn it on from an installed copy: `just mac-build`,
`just mac-install` (copies to `/Applications`), then launch
`/Applications/Quicuts.app` and enable it there. Turning it on from
`just mac-run` would point login at `target/debug`.

## Recipes

```bash
just mac-test    # proto + manifest + agent state-machine tests
just mac-agent   # build sidecar, stage into crates/quicuts-app/binaries/
just mac-ui      # pnpm build the frontend
just mac-run     # agent + ui + `cargo tauri dev` (dev loop; TCC → terminal)
just mac-build   # full .app bundle → target/release/bundle/macos/Quicuts.app (signed: § Signing)
just mac-signing-identity  # one-time: the self-signed local signing identity
just mac-install # copy the built bundle to /Applications (for launch at login)
just mac-log     # tail ~/Library/Logs/com.barbowza.quicuts/Quicuts.log
just mac-input   # post synthetic activation input — read the warning below first
just mac-menus   # dump a running app's menu shortcuts as TSV (manifest authoring)
```

## Gotchas

- **Stale sidecar:** tauri-build copies
  `binaries/quicuts-agent-aarch64-apple-darwin` over
  `target/debug/quicuts-agent` during app builds. `just mac-run` restages
  the agent first, so always go through it (a bare `cargo tauri dev` can
  run a stale agent).
- **Transparent overlay needs `macOSPrivateApi`:** the `macos-private-api`
  cargo feature on `tauri` plus `app.macOSPrivateApi: true` in the *base*
  `tauri.conf.json`. Don't drop either, and don't move the flag into
  `conf/macos.json` — `tauri-build` compares the cargo feature against the
  config with no idea what target it is building for, so a mac-only config
  overlay against an unconditional cargo feature breaks the Windows build
  (`features on the Cargo.toml file does not match the allowlist`). The flag
  is inert off macOS: the feature is empty and only gates `wkwebview` code.
- **Manifests:** the mac set lives in `manifests-mac/` (bundle-id
  `WindowFilter`s); `bundled_manifests_dir` prefers it on macOS. In dev it
  resolves through the workspace path, in bundles through
  `Contents/Resources/manifests-mac`.
- **Adding a manifest needs a config touch, not just a rebuild.**
  `cargo tauri dev` copies `manifests-mac/` into `target/debug/` keyed on the
  *config's* mtime, not the directory's contents, so a new `.yml` is not staged
  by editing Rust alone. `touch crates/quicuts-app/tauri.conf.json` to restage;
  the log line `loaded N bundled manifests` is the check.
- **Authoring a mac manifest:** if the app has no canonical shortcut article
  (or its article lags the shipped build), read the app's own menus with
  `just mac-menus <app>` — see *Reading an app's menus* below.
- **pnpm build scripts:** `ui/pnpm-workspace.yaml` allows esbuild's
  postinstall (`allowBuilds`). pnpm ≥ 11 ignores the old
  `package.json#pnpm` field; without the yaml the vite build fails.
- **Logs:** `just mac-log` in a second terminal. Agent stderr lines appear
  as `agent: …` at Info level.
- **Synthetic input can't be driven by AppleScript.** `osascript … keystroke
  "/" using {command down}` will *never* activate Quicuts. `tap.rs`'s
  `translate()` decides whether a modifier went down or up from the
  **device-dependent** `NX_DEVICE*` flag bits (`0x1` left ⌃, `0x2` left ⇧,
  `0x8` left ⌘ — see `device_bit()`), because the generic
  `CGEventFlags::maskCommand` bit stays set while *either* ⌘ is held and so
  cannot distinguish press from release. AppleScript sets only the generic
  bits, so the state machine reads every modifier as already released and
  nothing fires. To drive activation programmatically, post `CGEvent`s with
  **both** bit sets — e.g. for a ⌘ hold, a `.flagsChanged` event with
  keycode `0x37` and flags `maskCommand | 0x8`, then the same keycode with
  flags `0` to release. This is a property of the agent, not a bug: real
  hardware events always carry the device bits.

  `scripts/mac-synthetic-input.swift` does exactly that, and is the way to
  drive activation from a script: `just mac-input hold [ms] | holddown |
  cmdup | chord | esc`, or `just mac-input help`. It needs no dependencies
  beyond CoreGraphics, is compiled on the fly by the `swift` interpreter (no
  cargo, so it cannot affect any build), and inherits the same Accessibility
  grant the terminal already has for `just mac-run`. Its header carries the
  full explanation of the device bits.

  ⚠️ **Only run it on an idle machine with nothing important focused.**
  Synthetic events are posted at the session event tap, so they are delivered
  to whatever app currently has **focus** — Quicuts merely observes them in
  passing. A stray Esc dismisses whatever dialog or sheet is open; a stray
  keystroke replaces whatever an editor has selected. Either can destroy
  unsaved work, and nearly did on 2026-08-10. Check the frontmost app before
  every run, and prefer a scratch window. If you only need to exercise the
  overlay render path, the tray menu's **"Show shortcuts"** item
  (`tray.rs`) reaches `overlay::show` directly without posting a single
  event.

## Reading an app's menus (manifest authoring)

`just mac-menus <app-name-or-bundle-id>` dumps a **running** app's menu-bar
shortcuts as TSV, already normalised to the PTSG `Keys` grammar. It is the
source of truth for apps with no shortcut article, and for any app whose
article lags the installed build — a menu bar cannot lag. It is read-only
(Accessibility API), so unlike `mac-input` it posts no events and can never
disturb the app or lose work. It needs the same one-time Accessibility grant.

It emits TSV, not YAML, on purpose: which submenus deserve their own
`SectionName`, what earns `Recommended`, and how to word a name are judgement
calls. What the script guarantees is a correct *decode* — and there are two
traps that silently corrupt a hand-read:

- **The command bit is inverted.** `AXMenuItemCmdModifiers` has no "command"
  bit; `0x08` means *no* command, so a mask of `0` is plain ⌘. Read the obvious
  way round, every shortcut in the app comes out wrong while still looking
  plausible — and on macOS ⌘ is what sets the PTSG `Win` flag.
- **`0x10` is fn/Globe, and it also marks items that are not the app's.**
  macOS injects the Sequoia window-tiling block (Fill, Center, Move & Resize ▸
  …) into every app's Window menu, plus Dictation and Emoji & Symbols into
  Edit. They carry the fn bit, belong in `Apple.System` if anywhere, and import
  as nonsense like `⌃F` for "Fill". Such rows are tagged `system-fn`.

Neither flag catches the third hazard: **menus contain user data.** Window ▸
Arrangements lists arrangements saved on *this* Mac and Profiles lists the
user's profiles, both indistinguishable from stock rows. Read the titles before
committing.

Worth re-running against the manifest after authoring — reconciling the two
found three real iTerm2 shortcuts (`Dashboard`, `Select Matches`, `Maximize
Active Pane`) that a first pass had dropped, because their command character is
a literal carriage return that split the row in a line-based reader. The script
normalises control characters to `<Enter>` before printing for exactly that
reason.

## Autonomous mode

How to run this workflow with no human checkpoints, for a future unattended
session (verified observations, not guesses):

**Pre-grant once (human, ~30 seconds):** Accessibility for the terminal
app that hosts the session (System Settings → Privacy & Security →
Accessibility → iTerm/Terminal → on). Everything a session spawns from
that terminal — `cargo`, `cargo tauri dev`, the app, the agent sidecar —
inherits the terminal's TCC attribution, so this single grant covers
unlimited rebuilds. Confirmed empirically in this workflow: before the
grant the agent emitted `PermissionRequired` on every supervisor attempt;
after enabling iTerm in the Accessibility pane — changing nothing else,
same binaries — the next `just mac-run` reached `agent ready (proto 1)` on
attempt 0. No per-binary grant, no relaunch of iTerm, no stable signing
identity needed for the terminal loop (that mitigation only matters for
bundled .app builds, whose ad-hoc CDHash changes per rebuild).

**What a session can then do unattended:**
- build and test everything (`just mac-test`, `mac-agent`, `mac-build`);
- launch `just mac-run` and read the supervision log to confirm the agent
  reached `Ready` (proto 1, caps `hold`/`chord`/`foreground`) instead of a
  `Fatal` — this is observable from the terminal with no human;
- iterate on agent/app code and relaunch freely.

**What still needs a human:**
- clicking the TCC toggle itself — no command can grant it;
- judging what was actually *painted* — whether the panel looks right, not
  merely where it is (see below: geometry is checkable, rendering is not);
- a bundled .app's own grant: once per signing identity with the local
  identity (§ Signing), or once per rebuild for an ad-hoc bundle.

### Checking the panel without a human, and without screenshots

Split "did it work" into two questions. **Geometry and visibility are fully
checkable unattended, and should be the default** — no TCC grant beyond the
Accessibility one, and no privacy exposure:

```bash
# real frame of every Quicuts window, in points, top-left origin
CGWindowListCopyWindowInfo(.optionAll, kCGNullWindowID)
```

Filter on `kCGWindowOwnerName == "quicuts"` and read `kCGWindowBounds` plus
`kCGWindowIsOnscreen`. That is the *actual* frame macOS gave the window, so it
catches a panel that is mispositioned, mis-sized, or sitting outside every
display — which is exactly how the mixed-DPI placement bugs were found and
verified fixed. `.optionOnScreenOnly` **excludes** a window that is on no
display, so a window present under `.optionAll` but absent from the on-screen
list is itself the "shown but invisible" signal. Note `kCGWindowName` (the
title) needs Screen Recording; owner, bounds and the on-screen flag do not, so
filter by owner and never depend on titles.

**Only reach for `screencapture` when the question is genuinely about
rendering** — transparency, colours, glyphs, layout. It needs Screen Recording
on the terminal (a separate TCC bucket from Accessibility; Quicuts itself never
needs it), and it takes the *whole screen*, including whatever confidential
material happens to be visible. Never attach captures to PRs or artifacts, and
delete them when the check is done. Pixel-diffing before/after frames also only
isolates the overlay on an otherwise-idle screen: against a video call or a
busy editor, every changed bounding box is someone else's repaint.

⚠️ And the hazard that applies to *any* of this: synthetic input is delivered
to whatever app has **focus**, not to Quicuts — see the `just mac-input`
warning above. Check the frontmost app and what is unsaved before posting a
single event.
