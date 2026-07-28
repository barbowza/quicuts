# Glossary

Domain vocabulary for Quicuts. Terms here are the canonical names for use
in docs, settings UI copy, and code identifiers.

## Core

- **Collection** — one shortcut manifest's worth of content: the set of
  sections/shortcuts shown for one app (or web app). One manifest file =
  one collection.
- **Manifest** — the PTSG-compatible YAML file describing a collection
  (`<PackageName>.<locale>.yml`). Sources layer Bundled → PtsgRuntime →
  User; later source wins whole-file.
- **Rail** — the vertical strip of app icons in the overlay; one entry
  per matched collection. Order: exact match → hosted collections →
  wildcard → background.
- **Page** — the assembled sections of one collection, rendered when its
  rail entry is selected.
- **Pinned app** — a rail entry the user pinned: it stays listed and
  re-selects on every foreground change until unpinned. (Distinct from
  *pinned shortcuts*, the per-entry pins that populate a page's Pinned
  section.)
- **Activation** — showing the overlay, via **hold** (Win/Cmd held past
  the threshold) or **chord** (default Win+Shift+/). Chord activation
  keeps the panel up until dismissed — this is the "keep it on-screen
  while I work" mode.

## Hosted collections (ADR 0003)

- **Hosted collection** — a collection for an app that lives inside a
  host app rather than running as its own process, matched by host class
  instead of exe. Web apps in a browser (e.g. Gmail) are the first kind;
  "web app" is the browser-hosted specialization users see in UI copy.
- **Host** — the process a hosted collection appears under (Chrome,
  Firefox, Edge, …). Declared in the manifest via the `Host:` field.
- **Host class** — a named group of host executables that Quicuts owns
  centrally. `browser` is the only class today; the exe list is built-in
  and extensible in settings.
- **`TitleMatch`** — optional manifest field: a case-insensitive
  substring matched against the foreground window title to auto-detect
  the hosted app. Only consulted when the foreground exe is in the
  collection's host class, and only when experimental title detection is
  enabled in settings.
- **Title detection (experimental)** — the settings-gated feature where
  the agent watches foreground-window title changes and the app
  auto-selects a hosted collection whose `TitleMatch` hits. Fully live:
  selection follows the match in both directions; a pinned app overrides.

## Font scaling (ADR 0005)

- **Font scale** — the single accessibility zoom factor
  (`appearance.fontScale`, 80–200%, default 100%) sizing all text and
  keycaps inside the overlay window: panel, Help, Customize dialog, and
  rail. Settings window and taskbar badges do not scale. Adjusted by the
  settings slider (5% steps) or Ctrl+`=`/Ctrl+`-` (10% steps) and
  Ctrl+`0` (reset) while the overlay is focused.
- **Panel width** — the overlay's user-set width
  (`appearance.panelWidth`), changed by dragging the panel edge like a
  normal window. Minimum 586 logical px (the classic width), maximum
  half the display's width. Persists across sessions.
- **Auto width resize** — the settings toggle ("Resize panel with font
  size", off by default). On: effective width = panel width × font
  scale. Off (**manual**): font changes reflow content at the current
  width and only dragging moves the edge.
- **Focused-only hotkey** — the default scope for every Quicuts
  shortcut: it fires only while a Quicuts window has focus (webview
  keydown), never via the agent's global hook. Shortcuts are global only
  when explicitly designated.

## Unsupported apps (ADR 0004)

- **Unsupported app** — a foreground app whose exe is known but matches
  no collection as foreground (no exact, hosted, or non-background
  wildcard match). Background/wildcard collections like the Windows
  shell page do not make an app "supported".
- **Placeholder tile** — the dimmed rail entry inserted for an
  unsupported app: real exe icon, real name (version-info
  `FileDescription`, exe-stem fallback), synthetic id
  `unsupported:<exe>`, not pinnable. Its page is empty except
  "No shortcuts for <App>."
