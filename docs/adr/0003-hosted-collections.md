# ADR 0003 — Hosted collections (shortcuts for apps living inside a host app)

Status: accepted (2026-07-12). Extended by ADR 0007 (multi-pattern
`TitleMatch` + user title-signature bindings).

## Context

PTSG manifests match a shortcut collection to a foreground *process*
(`WindowFilter: chrome.exe`). But much daily work happens in web apps —
Gmail in Chrome or Firefox — whose shortcuts are not the browser's. The
user's target flow: while working in Gmail, summon Quicuts, see *Gmail's*
shortcuts, and keep the panel on-screen for reference while continuing to
work. In every other respect these collections should behave like Windows
app collections.

Codebase facts the design leans on:

- `ForegroundInfo` already carries the window `title` on every event; the
  app currently ignores it. Titles are window metadata, not keystrokes, so
  using them does not touch the privacy invariant (no new data class
  crosses the IPC boundary).
- The agent's foreground watcher hooks only `EVENT_SYSTEM_FOREGROUND` —
  it fires on window switches, **not** on tab switches inside a browser
  (those change the title of the same window, firing no event today).
- The existing **pinned app** mechanism (`set_pinned_app`) already keeps a
  rail app listed and re-selected across foreground changes, and
  chord-summon already keeps the panel visible while the user works. The
  "refer to Gmail while working in it" requirement is therefore served by
  existing machinery once Gmail can enter the rail.

## Decision

A **hosted collection** is a collection matched via a host class rather
than its own executable. Web apps in a browser are the first (and so far
only) kind.

### 1. Matching: `Host:` class field, not per-manifest exe lists

A hosted collection declares `Host: browser` (new optional manifest
field, a Quicuts extension to the PTSG schema). Quicuts owns one built-in
list of browser executables (chrome, msedge, firefox, brave, opera,
vivaldi, …), extensible in settings. A manifest with `Host:` needs no
`WindowFilter`. The class indirection means adding a browser is one
central change, and leaves room for future host classes (e.g. terminal).

### 2. Baseline interaction: manual selection + existing pinning

When a host-class exe is foreground, all installed hosted collections for
that class join the rail. Rail order: **host exact match → its hosted
collections → wildcard → background**; the *host* page stays selected by
default. The user clicks the hosted collection in the rail, and pins it
with the existing pin mechanism to keep it selected while working. No
heuristics in the baseline path.

### 3. Experimental auto-detection: title matching, off by default

Behind a settings toggle ("experimental"):

- The agent additionally watches the foreground window's **title**
  (`EVENT_OBJECT_NAMECHANGE` scoped to the foreground window), debounced,
  and only while the toggle is on. Title changes re-emit
  `ForegroundChanged` with the same window and new title — no new event
  type, no protocol data class beyond what already flows.
- The app matches titles against a new optional manifest field
  `TitleMatch:` — **case-insensitive substring** (no regex; keeps
  authoring trivial and the manifest crate dependency-free).
- `TitleMatch` is only consulted for collections whose `Host` class
  matches the foreground exe, so a Notepad document named "Gmail" can
  never trigger the Gmail collection.
- Behavior is **fully live**: match gained → the hosted collection
  auto-selects (and presents as the foreground rail app); match lost
  (e.g. tab switch) → selection reverts to the host. A pin overrides both
  directions, exactly as it does for window switches.

### 4. Icons

New optional `Icon:` manifest field naming an image file next to the
manifest (bundled Gmail ships one). Absent an icon, the rail renders a
generated letter tile from the collection name. No network fetches, ever.

### 5. Scope

- All host-matched collections show in the rail; per-collection
  visibility control is deferred until the installed set is large enough
  to crowd (we bundle only Gmail initially).
- Bundled content this milestone: **Google.Gmail only**, authored by us
  (PowerToys ships no web-app manifests), as the reference implementation.

## Consequences

- Schema gains three optional, backward-compatible fields (`Host`,
  `TitleMatch`, `Icon`); tolerant serde ignores them in tools that don't
  know them, and all existing manifests parse unchanged.
- `AgentCommand::Configure` grows a title-watch flag; the agent's hook
  surface grows only when the experimental toggle is on.
- `match_foreground` gains a host-class group between exact and wildcard;
  `engine::build_state` gains title-match selection and manifest-icon
  resolution.
- The privacy invariant is untouched: titles already crossed the wire;
  key identities still never do.

## Deferred / to verify on host

- Exact built-in browser exe list (and how settings extends it).
- Real-world Gmail title shapes across Chrome/Firefox/Edge to pick a
  robust `TitleMatch` value (Chrome: `Inbox (23) - user@gmail.com - Gmail`;
  Firefox appends `— Mozilla Firefox`).
- Title-change debounce interval (Gmail's unread count mutates the title).
- Per-collection rail visibility control (revisit when bundle grows).
