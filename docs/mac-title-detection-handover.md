# Handover: title detection + signature bindings on macOS

For: the Mac Claude session (see `docs/two-agent-review-process.md`).
Context: ADR 0007 shipped experimental title detection's user-facing half —
multi-pattern `TitleMatch` plus user signature bindings — Windows-only.
Everything decision-shaped is settled; this is the mac-side work to light
it up.

## What you inherit for free (do not rebuild)

- `ManifestStore::match_title` — bindings, precedence (user beats
  manifest, longest wins), host-class gating. Platform-free, fully
  unit-tested, runs in `just test` / `just mac-test`.
- `Settings::title_bindings` (+ `titleDetection`, `extraBrowserExes`) and
  the whole settings-UI capture flow (`Settings.svelte`, "Web app
  signatures"). The UI keys off data availability, not platform.
- `AppState.last_browser_title` upkeep in `agent.rs::apply_foreground` —
  fires for any foreground event whose exe/bundle-id is in the browser
  class and whose title is `Some`.

## What macOS still needs

1. **Browser class must learn bundle ids.** `BUILTIN_BROWSER_EXES` in
   `crates/quicuts-manifest/src/host.rs` holds `chrome`-style exe stems;
   the mac agent reports `com.google.Chrome`-style ids, which
   `HostClasses::contains` will never match. The set is just strings —
   add the bundle ids of the same browsers (`com.google.Chrome`,
   `com.apple.Safari`, `org.mozilla.firefox`, `com.microsoft.edgemac`,
   `company.thebrowser.Browser`, …). Mind `normalize_exe`: it lowercases
   and strips a trailing `.exe`, which is harmless for bundle ids, but
   verify nothing else mangles the dots.
2. **The mac agent needs a `title` capability.** `foreground.rs` currently
   sends `title: None` always (deliberate slice gap, ADR 0006). Needed:
   frontmost-window title at foreground change, plus title-change watching
   while `title_events_enabled` (the `Configure` flag the app already
   sends). The AX route (`AXObserver` on `kAXTitleChangedNotification` /
   `kAXFocusedWindowChanged`) needs the Accessibility grant the agent
   already holds for the event tap — but verify, on real hardware, that
   reading another app's window title works under the same TCC grant as
   the tap; advertise the `title` capability in `Ready{caps}` only when it
   does. Debounce like the Windows agent (`hook.rs` has the constant and
   the reasoning — Gmail unread-count churn).
3. **Safari-shaped titles.** Safari window titles don't append a browser
   suffix like "— Mozilla Firefox" / "- Google Chrome". The engine
   doesn't care (substring match), but `Settings.svelte::suggestPattern`
   drops the last dash-separated segment assuming it's the browser name —
   on Safari that would eat a real segment. Teach the heuristic per-title
   (e.g. only drop the tail when it names a known browser) rather than
   per-platform if you touch it.
4. **Update the ledgers.** When done: strike the hosted-collections bullet
   from ADR 0006's Known gaps, note the mac status in ADR 0007, and
   delete this file.

## Verification recipe (real Mac, TCC granted)

`just mac-test` first (binding matching is covered there), then
`just mac-run`: Safari/Chrome on consumer Gmail should auto-select the
Gmail collection with detection on; a Workspace account should do nothing
until you capture its org signature in Settings → Web apps → Web app
signatures, and auto-select afterwards. Esc/pin behavior must stay
identical to Windows (`next_selection` is shared and already tested).
