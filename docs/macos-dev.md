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

## Recipes

```bash
just mac-test    # proto + manifest + agent state-machine tests
just mac-agent   # build sidecar, stage into crates/quicuts-app/binaries/
just mac-ui      # pnpm build the frontend
just mac-run     # agent + ui + `cargo tauri dev` (dev loop; TCC → terminal)
just mac-build   # full .app bundle → target/release/bundle/macos/Quicuts.app
just mac-log     # tail ~/Library/Logs/com.barbowza.quicuts/Quicuts.log
```

## Gotchas

- **Stale sidecar:** tauri-build copies
  `binaries/quicuts-agent-aarch64-apple-darwin` over
  `target/debug/quicuts-agent` during app builds. `just mac-run` restages
  the agent first, so always go through it (a bare `cargo tauri dev` can
  run a stale agent).
- **Transparent overlay needs `macOSPrivateApi`:** supplied by
  `conf/macos.json` (merged via `--config` by the `mac-*` recipes) plus the
  `macos-private-api` cargo feature on `tauri`. Don't drop either.
- **Manifests:** the mac set lives in `manifests-mac/` (bundle-id
  `WindowFilter`s); `bundled_manifests_dir` prefers it on macOS. In dev it
  resolves through the workspace path, in bundles through
  `Contents/Resources/manifests-mac`.
- **pnpm build scripts:** `ui/pnpm-workspace.yaml` allows esbuild's
  postinstall (`allowBuilds`). pnpm ≥ 11 ignores the old
  `package.json#pnpm` field; without the yaml the vite build fails.
- **Logs:** `just mac-log` in a second terminal. Agent stderr lines appear
  as `agent: …` at Info level.

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
- *visual* verification (panel actually slides in, rail shows "Safari",
  ⌘Tab untouched): the logs prove events fired, not what was painted on
  screen. Treat log-level verification as "plumbing works" and keep
  on-screen claims out of reports unless a human confirmed them;
- anything involving a bundled .app's own grant (fresh grant per ad-hoc
  rebuild, or set up a stable Apple Development signing identity once to
  make bundle grants durable too).
