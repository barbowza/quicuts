# ADR 0007 — User title-signature bindings (and multi-pattern TitleMatch)

Status: accepted (2026-08-13). Extends ADR 0003 (hosted collections).

## Context

ADR 0003's experimental title detection matches a hosted collection via
`TitleMatch:`, a case-insensitive substring the manifest author ships.
That works only for apps whose titles are universal. Google Workspace
Gmail broke the assumption: its tab title ends in the *organization's*
name, not "Gmail" —

```
Inbox (17) - user@gmail.com - Gmail                        ← consumer: "- Gmail" is shippable
Inbox (7) - user@carbonregister.co.uk - Carbon Register Mail   ← per-organization signature
```

No bundled manifest can know every organization's name. Some apps have
**per-user signatures that only the user can supply.**

## Decision

### 1. `TitleMatch` becomes `string | [string]`

Tolerant serde (house style: `LaxBool`, `RawKeyToken`); existing manifests
parse unchanged. A manifest author can now ship several universal
signatures. `Manifest::title_match` is a `Vec<String>`.

### 2. A signature binding is user data in settings, not a manifest fork

`settings.json` gains `titleBindings: [{pattern, manifestId}]`
(`quicuts_manifest::TitleBinding`). A binding maps a case-insensitive
substring — same semantics as `TitleMatch`, deliberately one matching
concept — to an *installed hosted collection* by manifest id. Storing
bindings in settings (not a forked manifest in the User layer) means they
survive bundled-manifest updates; the User layer's whole-file-wins rule
would silently detach a forked Gmail from upstream fixes.

Manifests define *what collections exist*; bindings define *when one
auto-selects*.

### 3. Precedence is fixed and deterministic at match time

`ManifestStore::match_title` considers both pattern sources, gated
identically by host class (a binding can never fire outside a browser
window, and only for a manifest with `Host:` set). Resolution: **user
bindings beat manifest patterns, then longest pattern wins, ties by id.**
No runtime UI; the rail + pin remain the manual escape hatch. Bindings
naming an uninstalled or non-hosted manifest are inert — kept in settings,
flagged in the UI, never auto-deleted.

### 4. Capture happens in Settings, from the last-seen browser title

The app remembers the title of the most recent browser-class foreground
window (in-memory only). The settings "Web app signatures" UI — visible
only under the experimental title-detection toggle — shows that title,
pre-fills a suggested pattern (the segment before the trailing browser
name, which skips the user's email address), live-validates the pattern
against the title, offers installed hosted collections as targets, and
warns when the title also matches an existing pattern (the new binding
shadows it by rule 3). Saving an exact-duplicate pattern replaces the
older binding.

New commands: `get_last_browser_title`, `list_hosted_collections`.

## Consequences

- Workspace Gmail (and any per-org web app) is reachable today: bind
  `"Carbon Register Mail"` → `Google.Gmail`. New web apps need a user
  manifest with `Host: browser` first, then a binding — mechanism stays
  composable, and bundled-manifest coverage is a separate content track.
- No protocol change: `ForegroundChanged` already carried titles, and the
  privacy invariant is untouched (titles, never keystrokes; the persisted
  pattern is user-edited text).
- **Cross-platform since 2026-09-01** (issue #19). Matching lived in the
  platform-free crates all along, so macOS inherited the whole feature —
  bindings, precedence, the signatures UI — by adding two things and
  changing no shared logic: the mac agent gained a `title` capability, and
  the browser host class learned bundle ids
  (`BUILTIN_BROWSER_BUNDLE_IDS`). See ADR 0006 for the mac-side mechanics.
  One shared fix fell out of it: `suggestPattern` assumed the *last*
  dash-separated segment is the browser name, which is wrong on Safari
  (which appends nothing) and wrong on Chrome (which appends the profile
  name *after* the browser name — `"… - Gmail - Google Chrome – Work"`,
  observed live). It now cuts at the browser name instead of counting from
  the end, which fixes the Windows Chrome-with-profiles case too.
