# Quicuts — universal shortcuts guide

A cross-platform (Windows + macOS) shortcuts quick-finder modelled after
Windows PowerToys **Shortcut Guide**, built with Tauri v2. 

It shows a side-docked, app-aware panel of keyboard shortcuts when you 
hold Win/Cmd or press a hotkey, and imports PowerToys' YAML shortcut
manifests unchanged.

## Architecture

| Crate / dir | Role |
|---|---|
| `crates/quicuts-proto` | NDJSON IPC contract (app ↔ sidecar) |
| `crates/quicuts-manifest` | PTSG-compatible manifest engine (parse/index/match/assemble) — platform-free, unit-tested |
| `crates/quicuts-agent-win` | **Sidecar**: WH_KEYBOARD_LL hook, foreground watcher, taskbar reader. The only binary that touches system input, isolated so AV heuristics don't flag the main app. |
| `crates/quicuts-app` | Tauri host: tray, overlay/badges/settings windows, engine, agent supervisor, icon extraction |
| `ui/` | Svelte 5 + Vite frontend (overlay, badges, settings windows) |
| `manifests/` | Bundled PowerToys manifests (MIT — see `POWERTOYS-LICENSE`) |

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

See `docs/adr/` for the framework choice (0001) and the no-sudo cross-toolchain
(0002).


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

M1-level skeleton complete and verified booting on a real Windows host (34
manifests load, sidecar hook installs, NDJSON handshake works). Remaining:
on-host interactive verification of hold-to-show + Win-key suppression, then
M2–M5 per the plan.
