# ADR 0005 — Overlay font scaling (accessibility zoom)

Status: accepted (2026-07-28)

## Context

The overlay's type is small (11–15px) and every size in the UI is a
hard-coded `px` value: 44 `font-size` declarations across the Svelte
components, and fixed keycap boxes (`min-width: 24px; height: 26px` in
`KeyVisual.svelte`). Users who need larger text have no recourse short of
raising Windows' global text scaling, which affects everything else too.

Codebase facts the design leans on:

- `panelOpacity` already flows settings → `Appearance` struct →
  `emit_appearance` → `applyAppearance` in `theme.ts`, landing as a CSS
  variable in every window. A font scale can ride the same rails.
- The panel width is a Rust constant (`PANEL_WIDTH: f64 = 586.0`,
  `overlay.rs`); the window is not user-resizable today.
- Webview keydown only reaches a Quicuts window while it is focused;
  Ctrl+F (filter) and Ctrl+H (help) are hard-coded in `Overlay.svelte`'s
  window-level handler. Global chords would require agent-hook work.
- On this user's machine Windows text scaling (120%) already zooms the
  webviews invisibly to Rust; badge/overlay code divides physical
  coordinates by `devicePixelRatio`. Anything that distorts
  `getBoundingClientRect` values (like CSS `zoom`) endangers that math.
- The Customize dialog captures keydown in the capture phase while
  recording a binding, so window-level shortcuts cannot fire mid-capture.

## Decision

1. **One scale, text and keycaps together.** A single **font scale**
   (`appearance.fontScale`, default `1.0`) sizes everything inside the
   **overlay window**: panel text, keycap glyphs and boxes, Help, the
   Customize dialog, and the app rail (icons included). The settings
   window and taskbar badges do **not** scale — badges are
   geometry-locked to the taskbar rect, and the settings window is not an
   at-a-glance surface.
2. **Mechanism: CSS variable / rem, not CSS `zoom`.** `applyAppearance`
   drives a root font-size (or `--font-scale` factor); component sizes
   convert from `px` to `rem`-based values. CSS `zoom` is rejected
   because it distorts the client-rect values behind the physical-
   coordinate math (see Context).
3. **Range 80–200%, default 100%.** Hotkeys step by 10%, the settings
   slider by 5%; values clamp at the ends and persist as a clean number
   (e.g. `1.15`).
4. **Hotkeys: Ctrl+`=`/`+` (increase), Ctrl+`-` (decrease), Ctrl+`0`
   (reset to 100%).** Hard-coded like Ctrl+F/H, accepting main-row and
   numpad variants. **Focused-only** — this is now the standing default
   for every new Quicuts hotkey: shortcuts operate only while a Quicuts
   window is focused unless explicitly designated global. A sticky
   (unfocused) panel ignores them until clicked; that is accepted.
5. **Width is user-controlled, not font-controlled (by default).** The
   overlay becomes drag-resizable between **586 logical px** (minimum,
   today's width) and **half the display's width** (maximum). The dragged
   width persists (`appearance.panelWidth`, default 586). A settings
   toggle — **"Resize panel with font size"**, off by default — enables
   automatic mode, where the effective width is `panelWidth × fontScale`
   (same clamps). Manual is the default because the panel sits beside the
   user's working app; auto-widening would creep over it on zoom.
6. **Host is the source of truth.** The overlay's hotkeys invoke a
   command that clamps, persists, and rebroadcasts via the existing
   appearance event, so the settings slider, the overlay, and
   `settings.json` can never disagree.
7. **Settings UI: Appearance section.** "Font size" slider (live-updating,
   current % displayed) and the "Resize panel with font size" toggle sit
   alongside theme and panel opacity. The dragged width has no settings
   control — dragging *is* the control.

## Alternatives rejected

- **CSS `zoom` on the overlay body**: smallest diff, but changes
  client-rect coordinates that feed physical-coordinate math, and zooms
  layout the design wants stable (panel chrome, scrollbars).
- **Global Ctrl+±/0 via the agent hook**: expands the AV-sensitive
  sidecar surface, needs new protocol events, and collides with browser
  zoom — the most common Ctrl+± binding on the machine.
- **Auto-width as the default**: uniform zoom is prettier, but the
  panel's job is to sit beside the app being learned; silently widening
  over that app on every zoom-in punishes the primary workflow.
- **Scaling taskbar badges**: badge text lives inside
  taskbar-button-sized boxes computed from `IUIAutomation` rects;
  scaling clips before it helps.
- **Configurable zoom chords**: three more chord editors in settings for
  shortcuts whose whole value is matching universal muscle memory.

## Consequences

- `Appearance` (Rust + `types.ts`) gains `fontScale`, `panelWidth`,
  `autoWidthResize` — all `#[serde(default)]` so pre-existing
  `settings.json` files still parse (guarded by a compat test like ADR
  0003's).
- `overlay.rs` replaces the `PANEL_WIDTH` constant with
  settings-derived width, makes the window resizable with min/max
  constraints (max recomputed per-monitor), and persists the width on
  user resize (debounced).
- The overlay CSS migrates its `px` font sizes, keycap dimensions, and
  related gaps to `rem`-based sizing; content reflows/wraps at fixed
  width when the scale grows (KeyVisual already stacks alternative
  bindings vertically).
- New `#[tauri::command]`s (step/set font scale) registered in
  `generate_handler!` with wrappers in `ipc.ts`; `Overlay.svelte`'s
  keydown handler gains the three chords; Help's shortcut list documents
  them.
- Windows text scaling multiplies with the font scale (120% × 150% =
  180% effective) — acceptable; both are user intent.
