# Quicuts — universal shortcuts guide

A shortcuts quick-finder modelled after Windows PowerToys
**Shortcut Guide**, built with Tauri v2. Windows today as a daily driver; a
first macOS slice (hold-⌘ panel, app-aware matching by bundle ID) works too —
the platform-specific code sits behind an IPC seam for exactly that reason.

It shows a side-docked, app-aware panel of keyboard shortcuts when you
hold Win/Cmd or press a hotkey, and imports PowerToys' YAML shortcut
manifests unchanged.

## Features

- **Hold Win** (or a configurable hotkey chord) to show the panel; release or
  Esc to dismiss. A quick Win tap still opens the Start menu, and Win+E-style
  combos pass through untouched.
- **App-aware**: the panel shows the shortcuts for whatever app is in the
  foreground, with an app rail for browsing every loaded manifest.
- **Taskbar badges**: number pills over the taskbar buttons while the panel
  is up, matching the Win+1…9 launch shortcuts.
- **Pinning** and per-shortcut **customizations** (see below), stored in plain
  YAML separate from the bundled manifests.
- **Web apps (experimental)**: title-based detection of hosted apps like Gmail
  running inside a browser tab.
- 36 bundled manifests imported from PowerToys, plus any you drop in from a
  PowerToys installation or write yourself. macOS uses its own smaller set in
  `manifests-mac/`, matched by bundle ID.

Windows-only for now: taskbar badges (macOS has no ⌘1–9 Dock switching to
badge), app icons in the rail, and web-app title detection.

## Architecture

| Crate / dir | Role |
|---|---|
| `crates/quicuts-proto` | NDJSON IPC contract (app ↔ sidecar) |
| `crates/quicuts-manifest` | PTSG-compatible manifest engine (parse/index/match/assemble) — platform-free, unit-tested |
| `crates/quicuts-agent-win` | **Sidecar**: WH_KEYBOARD_LL hook, foreground watcher, taskbar reader. The only binary that touches system input, isolated so AV heuristics don't flag the main app. |
| `crates/quicuts-agent-mac` | **Sidecar**: CGEventTap + NSWorkspace frontmost watcher — the only macOS binary that touches system input. Same IPC contract; its hold/chord state machine is pure and unit-tested. |
| `crates/quicuts-app` | Tauri host: tray, overlay/badges/settings windows, engine, agent supervisor, icon extraction |
| `ui/` | Svelte 5 + Vite frontend (overlay, badges, settings windows) |
| `manifests/` | Bundled PowerToys manifests (MIT — see `POWERTOYS-LICENSE`) |
| `manifests-mac/` | macOS manifest set, matched by bundle ID |

## Build & run (WSL2 → Windows host)

One-time toolchain setup (Rust + cargo-xwin + clang-cl/llvm-rc):

```bash
./scripts/setup-wsl-toolchain.sh
```

Then:

```bash
just test          # fast Rust/manifest tests on the Linux toolchain
WINUSER=<you> just run    # cross-build, deploy to %LOCALAPPDATA%\QuicutsDev, launch on host
just kill          # stop it
just dev-server    # + `just dev-build` in another shell for frontend HMR
```

## Build & run (macOS)

Built natively on the Mac — no cross-compilation. The event tap needs a
one-time **Accessibility** grant in System Settings (in dev it is attributed to
the terminal you launch from, so granting once covers every rebuild).

```bash
just mac-test      # Rust/manifest/state-machine tests
just mac-run       # build agent + ui, run the dev app
just mac-build     # full .app bundle
just mac-log       # tail the log
```

Setup, dev loop, and gotchas: `docs/macos-dev.md`. Decisions and known gaps:
`docs/adr/0006-macos-agent.md`.

See `docs/adr/` for the architecture decision records: the framework choice
(0001), the no-sudo cross-toolchain (0002), hosted web-app collections (0003),
the unsupported-app placeholder (0004), overlay font scaling (0005), and the
macOS agent (0006).


### Running directly from Windows (restart Quicuts)

The app is self-contained once deployed — it can be relaunched directly from Windows CMD without rebuilding:
win cmd: `C:\Users\<you>\AppData\Local\QuicutsDev\quicuts.exe` 


wsl zsh: `cmd.exe /c start "" "C:\Users\<you>\AppData\Local\QuicutsDev\quicuts.exe"` 


## Customizing shortcuts

Quicuts documents shortcuts — it never remaps keys. If you've rebound keys
inside an app (say VSCode), record that in Quicuts so the panel shows *your*
bindings, not just the stock ones.

### In the panel

**Double-click any shortcut** to open its customization dialog. From there:

- **＋ Add customization** — a capture box opens; type the key combination
  exactly as you'd press it. Multi-chord sequences work naturally: press
  `Ctrl+K` then `Z` and both chords are recorded. **Enter** saves,
  **Backspace** removes the last chord, **Esc** cancels. The physical Win key
  is reserved by the system, so to record a Win-based combo tick the **⊞**
  toggle in the capture box before pressing the rest of the chord.
- **✕** next to a customization removes it.
- **Reassigned** checkbox next to a default binding — tick it when you've
  rebound that key in the app itself, so Quicuts shows the default as no
  longer available.

Keycap border colors tell you where a binding comes from:

| Border | Meaning |
|---|---|
| **Green** | Default from the app's manifest |
| **Yellow** | Your customization |
| **Mid-gray** (dimmed) | A default you marked as reassigned |

Customizations stack above defaults with the usual "or" separators. The **⌨
switch** in the panel footer (next to Settings) controls which bindings are
shown — click it to cycle: **Defaults → Custom → All → Custom ▸ defaults**
(the last one shows your bindings where they exist and defaults everywhere
else). The setting is remembered between runs.

### The customization files

Your customizations live in plain YAML you can edit with any text editor —
one file per app, separate from the bundled manifests (which Quicuts may
update; your files are never touched):

```
%APPDATA%\com.barbowza.quicuts\customizations\<App>.custom.yml
```

e.g. `Microsoft.VisualStudioCode.custom.yml`:

```yaml
Toggle Zen Mode:
  custom: ["Ctrl+Alt+Z"]      # bindings you added
  redefined: ["Ctrl+K Z"]     # defaults you reassigned in the app
```

Format rules:

- The top-level keys are shortcut names exactly as shown in the panel.
- Keys within a chord are joined with `+`; chords in a sequence are separated
  by spaces (`Ctrl+K Z` = press Ctrl+K, then Z).
- Modifiers: `Win`, `Ctrl`, `Shift`, `Alt`. Named keys like `Up`, `Enter`,
  `PageDown`, `F5` work; spelling is forgiving (case doesn't matter,
  `Esc`/`Escape` and `Enter`/`<Enter>` are equivalent). Use `Plus` for the
  literal `+` key.
- Edits are picked up the next time the panel refreshes — no restart needed.
  A line Quicuts can't parse is skipped (never an error), so a typo just
  means that binding won't show until fixed.

## Status

Working daily-driver on Windows: hold-to-show with Win-key suppression,
app-aware panels, taskbar badges, pinning, and customizations are all in use
on a real host. Experimental web-app title detection is implemented behind a
settings toggle. The macOS agent is not yet started — the IPC protocol is the
seam it will plug into.

## License

MIT — see `LICENSE`. The bundled manifests are from PowerToys, also MIT
(`manifests/POWERTOYS-LICENSE`, `NOTICE.txt`).
