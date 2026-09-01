# ADR 0004 — Unsupported-app placeholder in the rail

Status: accepted (2026-07-13)

## Context

When the foreground app matches no collection (e.g. Windows Calculator,
Windows Media Player), the overlay today shows nothing app-specific and
silently falls back to the first rail entry — the `+WindowsNT.Shell`
"Windows" page (`WindowFilter: "*"` + `BackgroundProcess: true`). No rail
tile is marked foreground (all remaining matches are
`MatchKind::Background`), so the panel reads as if Windows shell
shortcuts *were* the app's shortcuts. Users can't tell "this app has no
collection" from "the panel is stale".

Codebase facts the design leans on:

- `ForegroundInfo` already carries `exe_name` and `exe_path`; the icon
  pipeline (`icons::icon_data_uri`) can already render the real app icon.
- Rail identity, selection, pinning, and customization all key off
  `RailApp.manifest_id`; a placeholder needs a synthetic id that those
  paths must not treat as a manifest.
- Customization is only reachable by double-clicking an existing shortcut
  row, so an empty page keeps it unreachable with no extra guarding.
- Frame-hosted UWP apps (Calculator, Media Player on some builds) put
  `ApplicationFrameHost.exe` in the foreground; the real app owns a
  `Windows.UI.Core.CoreWindow` child inside the frame. The watcher
  unwraps this (`foreground::uwp_child_pid`) and reports the hosted
  app's process. If the frame is empty (suspended app, or a race right
  after launch before the CoreWindow reparents), it falls back to the
  frame host.

## Decision

When a foreground exe is **known** but **no rail tile would be marked
foreground** (no Exact and no non-background Wildcard match), the engine
inserts a **placeholder tile** at the front of the rail:

> **Amended 2026-09-01.** As accepted, this rule read "no Exact, no
> Hosted, no non-background Wildcard match" — a *Hosted* match suppressed
> the placeholder and took the foreground slot. That holds only while the
> host app has a manifest of its own, in which case an Exact match exists
> anyway. When it does not, the rule handed the foreground slot to a
> hosted collection: Gmail's shortcuts presented as the foreground app on
> a blank Firefox new-tab page, at every URL, which is precisely the
> "reads as if those were the app's shortcuts" failure in the Context
> above. `engine::build_state` implemented this rule faithfully, so the
> defect was in the decision, not the code. Hosted is now excluded: a
> hosted collection claims the foreground slot only when title detection
> matched it. See the correction in ADR 0003 and
> `quicuts_manifest::foreground_entry`.

1. **Real identity, not generic.** The tile shows the app's real exe
   icon and its version-info `FileDescription` ("Windows Calculator"),
   falling back to the cleaned exe stem ("CalculatorApp"). The
   FileDescription read lives in `quicuts-app` (file metadata, not input
   access — the sidecar boundary is untouched).
2. **Dimmed visual treatment.** Reduced opacity / muted label signals
   "no shortcuts here"; no new iconography (corner badges get murky at
   120% text scaling).
3. **Rail keeps everything else.** The Windows tile and live background
   apps (PowerToys, Telegram, …) stay exactly as today; the placeholder
   only occupies the foreground slot.
4. **Placeholder page is auto-selected** on switch (the existing rule:
   pinned app wins, else foreground). Its page is empty except a
   centered message: **"No shortcuts for \<App\>."** — message only, no
   further hints.
5. **No pin affordance** on the placeholder. Pins persist manifest ids
   across sessions; a pinned synthetic id would resurrect an empty page
   forever. Click-to-select only.
6. **Synthetic id** `unsupported:<normalized-exe>` (per-exe, so
   selection resets naturally between different unsupported apps). Never
   persisted; excluded from pin/customize paths.
7. **Exe unknown** (desktop focused, elevated/secure window): behavior
   unchanged — no placeholder; "Unsupported: \<nothing\>" is
   meaningless and would flash during Alt+Tab.

## Alternatives rejected

- **Generic "Unsupported App" tile** (the original proposal): loses
  which app you're looking at; two unsupported apps look identical.
- **Keeping the Windows page selected** with the placeholder unselected:
  preserves the stale-looking-page problem this feature exists to fix.
  (Re-raised in review 2026-09-01 for browsers Quicuts ships no manifest
  for — Brave, Opera, Vivaldi, Arc — on the grounds that the system page
  is sitting right there in the rail, excluded from the default only
  because `BackgroundProcess: true` outranks `"*"` in the grouping. Still
  rejected, for the reason above: it is the original bug, and an
  unsupported browser is not a special case of unsupported app.)
- **Empty-state hint pointing at user manifests**: an undocumented
  power-user path today; belongs in Help if user manifests become a
  documented feature.

## Consequences

- `RailApp` grows an `unsupported: bool` (mirrored in `ui/src/lib/types.ts`);
  `AppRail` dims the tile and hides the pin affordance; `Overlay` renders
  the empty-state message for an empty unsupported page.
- `engine::build_state` gains the placeholder branch; a FileDescription
  reader joins the icon helpers.
- Badges stay dark while the placeholder page is selected (`has_taskbar`
  is false for an empty page) — consistent with "clear the shortcuts
  list".
- UWP exes live under `WindowsApps`; their version-info/icon reads may
  be ACL-denied, in which case the tile falls back to the exe stem and
  letter tile — still the right app, just plainer.
