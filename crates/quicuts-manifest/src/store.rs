//! Manifest loading, source layering, in-memory index, and foreground
//! matching. PTSG's `index.yml` is deliberately ignored — it is a cache
//! regenerable from the folder; we rebuild the equivalent in memory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{normalize_exe, parse_manifest, split_filename, HostClasses, Manifest, TitleBinding};

/// Where a manifest came from. Later sources override earlier ones for the
/// same `(PackageName, locale)` — whole-file override, no per-entry merging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceKind {
    /// Manifests shipped with Quicuts (copied from PowerToys, MIT).
    Bundled,
    /// PTSG's runtime folder: %LOCALAPPDATA%\Microsoft\WinGet\KeyboardShortcuts
    PtsgRuntime,
    /// User-authored manifests in Quicuts' config dir.
    User,
}

#[derive(Debug, Clone)]
pub struct LoadedManifest {
    pub manifest: Manifest,
    pub source: SourceKind,
    pub locale: String,
    pub path: PathBuf,
}

/// Which matching rule admitted a manifest into the rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// `window_filter` equals the foreground exe.
    Exact,
    /// Hosted collection whose host class contains the foreground exe.
    Hosted,
    /// `window_filter` is "*".
    Wildcard,
    /// `BackgroundProcess: true` manifest (always-on or process-alive).
    Background,
}

#[derive(Debug, Clone, Copy)]
pub struct Matched<'a> {
    pub kind: MatchKind,
    pub lm: &'a LoadedManifest,
}

#[derive(Debug, Default)]
pub struct ManifestStore {
    /// (package_id, locale) -> best manifest seen so far.
    by_id: HashMap<(String, String), LoadedManifest>,
}

impl ManifestStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load every `*.yml`/`*.yaml` in `dir`. Files that fail to parse are
    /// logged and skipped — a broken manifest never takes the engine down.
    /// Returns (loaded, failed) counts.
    pub fn load_dir(&mut self, dir: &Path, source: SourceKind) -> (usize, usize) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                log::debug!("manifest dir {} not readable: {e}", dir.display());
                return (0, 0);
            }
        };
        let (mut ok, mut failed) = (0, 0);
        let mut paths: Vec<PathBuf> =
            entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for path in paths {
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some((pkg, locale)) = split_filename(file_name) else {
                continue;
            };
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|s| parse_manifest(&s, &pkg).map_err(|e| e.to_string()))
            {
                Ok(manifest) => {
                    let key = (manifest.id.clone(), locale.clone());
                    let candidate = LoadedManifest { manifest, source, locale, path };
                    match self.by_id.get(&key) {
                        // Whole-file override: same-or-later source wins.
                        Some(existing) if existing.source > candidate.source => {}
                        _ => {
                            self.by_id.insert(key, candidate);
                        }
                    }
                    ok += 1;
                }
                Err(e) => {
                    log::warn!("skipping manifest {}: {e}", path.display());
                    failed += 1;
                }
            }
        }
        (ok, failed)
    }

    pub fn clear(&mut self) {
        self.by_id.clear();
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Pick the best locale variant of each package for `ui_locale`
    /// (exact -> same language -> en-US -> any), then return them all.
    fn localized(&self, ui_locale: &str) -> Vec<&LoadedManifest> {
        let ui_lang = ui_locale.split('-').next().unwrap_or(ui_locale);
        let mut best: HashMap<&str, (&LoadedManifest, u8)> = HashMap::new();
        for lm in self.by_id.values() {
            let rank = if lm.locale.eq_ignore_ascii_case(ui_locale) {
                3
            } else if lm
                .locale
                .split('-')
                .next()
                .map(|l| l.eq_ignore_ascii_case(ui_lang))
                .unwrap_or(false)
            {
                2
            } else if lm.locale.eq_ignore_ascii_case("en-US") {
                1
            } else {
                0
            };
            match best.get(lm.manifest.id.as_str()) {
                Some((_, r)) if *r >= rank => {}
                _ => {
                    best.insert(&lm.manifest.id, (lm, rank));
                }
            }
        }
        let mut v: Vec<&LoadedManifest> = best.into_values().map(|(lm, _)| lm).collect();
        v.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        v
    }

    pub fn get(&self, id: &str, ui_locale: &str) -> Option<&LoadedManifest> {
        self.localized(ui_locale)
            .into_iter()
            .find(|lm| lm.manifest.id == id)
    }

    /// PTSG matching rules for a foreground exe, extended with hosted
    /// collections (ADR 0003): rail order = exact filter matches, then
    /// hosted collections whose host class contains the foreground exe,
    /// then "*" non-background, then background manifests ("*" background
    /// = always shown, e.g. Shell; named background manifests are included
    /// only if `running_check` says their process is alive). A manifest
    /// with `host` set matches only via its host class — its
    /// `window_filter` never admits it to the other groups.
    pub fn match_foreground(
        &self,
        foreground_exe: Option<&str>,
        ui_locale: &str,
        host_classes: &HostClasses,
        mut running_check: impl FnMut(&str) -> bool,
    ) -> Vec<Matched<'_>> {
        let fg = foreground_exe.map(normalize_exe);
        let all = self.localized(ui_locale);
        let mut exact = Vec::new();
        let mut hosted = Vec::new();
        let mut wildcard = Vec::new();
        let mut background = Vec::new();
        for lm in all {
            let m = &lm.manifest;
            if let Some(class) = &m.host {
                if let Some(fg_raw) = foreground_exe {
                    if host_classes.contains(class, fg_raw) {
                        hosted.push(lm);
                    }
                }
            } else if m.background_process {
                // Any one of its identities running is enough.
                if m.is_wildcard()
                    || m.window_filters.iter().any(|f| running_check(&normalize_exe(f)))
                {
                    background.push(lm);
                }
            } else if m.is_wildcard() {
                wildcard.push(lm);
            } else if let Some(fg) = &fg {
                if m.matches_identity(fg) {
                    exact.push(lm);
                }
            }
        }
        // `localized()` iterates a HashMap, so sort each group for a stable
        // rail order; among background manifests the always-on "*" ones
        // (Windows shell) go last.
        exact.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        hosted.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        wildcard.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        background.sort_by(|a, b| {
            let a_star = a.manifest.is_wildcard();
            let b_star = b.manifest.is_wildcard();
            a_star.cmp(&b_star).then_with(|| a.manifest.id.cmp(&b.manifest.id))
        });
        fn tag<'a>(
            v: Vec<&'a LoadedManifest>,
            kind: MatchKind,
        ) -> impl Iterator<Item = Matched<'a>> {
            v.into_iter().map(move |lm| Matched { kind, lm })
        }
        tag(exact, MatchKind::Exact)
            .chain(tag(hosted, MatchKind::Hosted))
            .chain(tag(wildcard, MatchKind::Wildcard))
            .chain(tag(background, MatchKind::Background))
            .collect()
    }

    /// All hosted collections (manifests with `Host:` set) for a UI locale,
    /// sorted by id. The settings UI offers these as binding targets.
    pub fn hosted(&self, ui_locale: &str) -> Vec<&LoadedManifest> {
        self.localized(ui_locale)
            .into_iter()
            .filter(|lm| lm.manifest.host.is_some())
            .collect()
    }

    /// Best hosted collection for a foreground title, from two pattern
    /// sources: the manifests' own `TitleMatch` patterns and the user's
    /// signature bindings (both case-insensitive substrings). Precedence:
    /// user bindings beat manifest patterns, then longest pattern wins
    /// (most specific), ties broken by id. Both sources are restricted to
    /// collections whose host class contains `foreground_exe` — a binding
    /// naming a non-hosted or uninstalled manifest is inert. Used by
    /// experimental title detection only.
    pub fn match_title(
        &self,
        foreground_exe: Option<&str>,
        title: Option<&str>,
        ui_locale: &str,
        host_classes: &HostClasses,
        bindings: &[TitleBinding],
    ) -> Option<&LoadedManifest> {
        let fg = foreground_exe?;
        let title = title?.to_lowercase();
        // (is_user_binding, pattern_len, manifest) per hit.
        let mut candidates: Vec<(bool, usize, &LoadedManifest)> = Vec::new();
        for lm in self.localized(ui_locale) {
            let Some(class) = lm.manifest.host.as_deref() else { continue };
            if !host_classes.contains(class, fg) {
                continue;
            }
            let manifest_patterns =
                lm.manifest.title_match.iter().map(|p| (false, p.as_str()));
            let binding_patterns = bindings
                .iter()
                .filter(|b| b.manifest_id == lm.manifest.id)
                .map(|b| (true, b.pattern.as_str()));
            for (is_user, pattern) in manifest_patterns.chain(binding_patterns) {
                let pattern = pattern.trim();
                if !pattern.is_empty() && title.contains(&pattern.to_lowercase()) {
                    candidates.push((is_user, pattern.len(), lm));
                }
            }
        }
        candidates
            .into_iter()
            .max_by(|(a_user, a_len, a), (b_user, b_len, b)| {
                a_user
                    .cmp(b_user)
                    .then(a_len.cmp(b_len))
                    .then_with(|| b.manifest.id.cmp(&a.manifest.id))
            })
            .map(|(_, _, lm)| lm)
    }
}

/// Which entry of a `match_foreground` result presents as "the foreground
/// app" — the one whose page is shown by default and whose name labels the
/// panel. `None` means nothing matched, and the caller shows its
/// unsupported-app placeholder.
///
/// 1. The title-matched hosted collection, when detection has one. That is
///    the point of ADR 0003: while you are in the Gmail tab, Gmail *is* the
///    app you are in.
/// 2. Otherwise the first **exact or wildcard** match — never a hosted one.
///
/// Rule 2's exclusion is the load-bearing half. Groups are ordered exact →
/// hosted → wildcard → background, so "first non-background" — which this
/// used to be — picks a hosted collection whenever the foreground app has
/// no manifest of its own but *is* a host. The symptom was Gmail's
/// shortcuts presenting as the foreground app on a blank Firefox new-tab
/// page, at every URL, with nothing in the panel to say nothing matched.
///
/// **Note on the wildcard arm:** no bundled manifest currently reaches
/// `MatchKind::Wildcard`. Both `"*"` manifests (`+WindowsNT.Shell`,
/// `Apple.System`) are also `BackgroundProcess: true`, and `match_foreground`
/// tests `background_process` first, so they land in `Background`. In
/// practice today rule 2 therefore resolves to "the first exact match, else
/// the placeholder". The arm is kept because this function specifies a rule
/// over `MatchKind`, not over the manifests that happen to ship — a user
/// manifest with `"*"` and no `BackgroundProcess` produces one right now.
/// Whether a `"*"` *background* manifest should also be eligible here is a
/// separate behaviour question, deliberately not decided by this function.
pub fn foreground_entry(matched: &[Matched<'_>], title_matched: Option<&str>) -> Option<usize> {
    title_matched
        .and_then(|t| {
            matched
                .iter()
                .position(|m| m.kind == MatchKind::Hosted && m.lm.manifest.id == t)
        })
        .or_else(|| {
            matched
                .iter()
                .position(|m| matches!(m.kind, MatchKind::Exact | MatchKind::Wildcard))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).unwrap();
    }

    fn mk(id: &str, filter: &str, bg: bool, extra: &str) -> String {
        format!(
            "PackageName: {id}\nName: {id}\nWindowFilter: \"{filter}\"\nBackgroundProcess: {bg}\n{extra}Shortcuts:\n  - SectionName: S\n    Properties:\n      - Name: E\n        Shortcut:\n        - Ctrl: true\n          Keys:\n            - A\n"
        )
    }

    #[test]
    fn layering_later_source_wins() {
        let t = std::env::temp_dir().join(format!("quicuts-store-{}", std::process::id()));
        let bundled = t.join("bundled");
        let user = t.join("user");
        fs::create_dir_all(&bundled).unwrap();
        fs::create_dir_all(&user).unwrap();
        write(&bundled, "A.App.en-US.yml", &mk("A.App", "a.exe", false, ""));
        write(&user, "A.App.en-US.yml", &mk("A.App", "different.exe", false, ""));

        let mut store = ManifestStore::new();
        store.load_dir(&bundled, SourceKind::Bundled);
        store.load_dir(&user, SourceKind::User);
        let lm = store.get("A.App", "en-US").unwrap();
        assert_eq!(lm.source, SourceKind::User);
        assert_eq!(lm.manifest.window_filters, vec!["different.exe"]);

        // Reloading bundled after user must NOT downgrade.
        store.load_dir(&bundled, SourceKind::Bundled);
        assert_eq!(store.get("A.App", "en-US").unwrap().source, SourceKind::User);
        fs::remove_dir_all(&t).ok();
    }

    fn mk_hosted(id: &str, title_match: Option<&str>) -> String {
        let tm = title_match
            .map(|t| format!("TitleMatch: \"{t}\"\n"))
            .unwrap_or_default();
        format!(
            "PackageName: {id}\nName: {id}\nHost: browser\n{tm}Shortcuts:\n  - SectionName: S\n    Properties:\n      - Name: E\n        Shortcut:\n        - Ctrl: true\n          Keys:\n            - A\n"
        )
    }

    fn ids<'a>(matched: &'a [Matched<'a>]) -> Vec<&'a str> {
        matched.iter().map(|m| m.lm.manifest.id.as_str()).collect()
    }

    #[test]
    fn matching_rules() {
        let t = std::env::temp_dir().join(format!("quicuts-match-{}", std::process::id()));
        fs::create_dir_all(&t).unwrap();
        write(&t, "Shell.en-US.yml", &mk("Shell", "*", true, ""));
        write(&t, "Chrome.en-US.yml", &mk("Chrome", "chrome.exe", false, ""));
        write(&t, "Bg.en-US.yml", &mk("Bg", "telegram.exe", true, ""));

        let mut store = ManifestStore::new();
        store.load_dir(&t, SourceKind::Bundled);
        let hc = HostClasses::builtin();

        // Foreground chrome, telegram not running: Chrome then Shell.
        let matched = store.match_foreground(Some("CHROME.EXE"), "en-US", &hc, |_| false);
        assert_eq!(ids(&matched), vec!["Chrome", "Shell"]);
        assert_eq!(matched[0].kind, MatchKind::Exact);
        assert_eq!(matched[1].kind, MatchKind::Background);

        // Telegram running: background manifest joins.
        let matched =
            store.match_foreground(Some("chrome"), "en-US", &hc, |exe| exe == "telegram");
        assert_eq!(ids(&matched), vec!["Chrome", "Bg", "Shell"]);

        // Unknown foreground: only always-on Shell.
        let matched = store.match_foreground(Some("unknown.exe"), "en-US", &hc, |_| false);
        assert_eq!(ids(&matched), vec!["Shell"]);
        fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn hosted_matching() {
        let t = std::env::temp_dir().join(format!("quicuts-hosted-{}", std::process::id()));
        fs::create_dir_all(&t).unwrap();
        write(&t, "Shell.en-US.yml", &mk("Shell", "*", true, ""));
        write(&t, "Google.Chrome.en-US.yml", &mk("Google.Chrome", "chrome.exe", false, ""));
        write(&t, "Google.Gmail.en-US.yml", &mk_hosted("Google.Gmail", Some("- Gmail")));
        write(&t, "Acme.Notion.en-US.yml", &mk_hosted("Acme.Notion", None));

        let mut store = ManifestStore::new();
        store.load_dir(&t, SourceKind::Bundled);
        let hc = HostClasses::builtin();

        // Browser foreground: exact host first, hosted collections next
        // (sorted by id), background last.
        let matched = store.match_foreground(Some("chrome.exe"), "en-US", &hc, |_| false);
        assert_eq!(ids(&matched), vec!["Google.Chrome", "Acme.Notion", "Google.Gmail", "Shell"]);
        assert_eq!(matched[1].kind, MatchKind::Hosted);
        assert_eq!(matched[2].kind, MatchKind::Hosted);

        // Hosted collections match any browser in the class, not just Chrome.
        let matched = store.match_foreground(Some("firefox.exe"), "en-US", &hc, |_| false);
        assert_eq!(ids(&matched), vec!["Acme.Notion", "Google.Gmail", "Shell"]);

        // Non-browser foreground: hosted collections never appear.
        let matched = store.match_foreground(Some("notepad.exe"), "en-US", &hc, |_| false);
        assert_eq!(ids(&matched), vec!["Shell"]);

        // No foreground at all: hosted collections never appear.
        let matched = store.match_foreground(None, "en-US", &hc, |_| false);
        assert_eq!(ids(&matched), vec!["Shell"]);

        // A settings-extended browser exe admits hosted collections too.
        let hc_ext = HostClasses::with_extensions(&["Thorium.exe".into()]);
        let matched = store.match_foreground(Some("thorium.exe"), "en-US", &hc_ext, |_| false);
        assert_eq!(ids(&matched), vec!["Acme.Notion", "Google.Gmail", "Shell"]);
        fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn hosted_never_leaks_into_other_groups() {
        let t = std::env::temp_dir().join(format!("quicuts-leak-{}", std::process::id()));
        fs::create_dir_all(&t).unwrap();
        // A hosted manifest that also (wrongly) carries a wildcard filter
        // and background flag: host wins, other rules never see it.
        write(
            &t,
            "Odd.App.en-US.yml",
            "PackageName: Odd.App\nHost: browser\nWindowFilter: \"*\"\nBackgroundProcess: true\nShortcuts:\n  - SectionName: S\n    Properties:\n      - Name: E\n        Shortcut:\n        - Keys:\n            - A\n",
        );
        let mut store = ManifestStore::new();
        store.load_dir(&t, SourceKind::Bundled);
        let hc = HostClasses::builtin();

        let matched = store.match_foreground(Some("notepad.exe"), "en-US", &hc, |_| true);
        assert!(matched.is_empty());
        let matched = store.match_foreground(Some("chrome.exe"), "en-US", &hc, |_| true);
        assert_eq!(ids(&matched), vec!["Odd.App"]);
        assert_eq!(matched[0].kind, MatchKind::Hosted);
        fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn title_matching() {
        let t = std::env::temp_dir().join(format!("quicuts-title-{}", std::process::id()));
        fs::create_dir_all(&t).unwrap();
        write(&t, "Google.Gmail.en-US.yml", &mk_hosted("Google.Gmail", Some("- Gmail")));
        write(&t, "Acme.Mail.en-US.yml", &mk_hosted("Acme.Mail", Some("mail")));
        write(&t, "Acme.NoPattern.en-US.yml", &mk_hosted("Acme.NoPattern", None));

        let mut store = ManifestStore::new();
        store.load_dir(&t, SourceKind::Bundled);
        let hc = HostClasses::builtin();

        // Case-insensitive substring; longest pattern wins over "mail".
        let hit = store.match_title(
            Some("chrome.exe"),
            Some("Inbox (3) - a@b.com - GMAIL"),
            "en-US",
            &hc,
            &[],
        );
        assert_eq!(hit.unwrap().manifest.id, "Google.Gmail");

        // Only the shorter pattern hits.
        let hit =
            store.match_title(Some("firefox.exe"), Some("Fastmail: Inbox"), "en-US", &hc, &[]);
        assert_eq!(hit.unwrap().manifest.id, "Acme.Mail");

        // Host-class gating: a non-browser window titled like Gmail never hits.
        assert!(store
            .match_title(Some("notepad.exe"), Some("x - Gmail"), "en-US", &hc, &[])
            .is_none());

        // Missing exe or title: no match.
        assert!(store.match_title(None, Some("x - Gmail"), "en-US", &hc, &[]).is_none());
        assert!(store.match_title(Some("chrome.exe"), None, "en-US", &hc, &[]).is_none());

        // Unmatched title: no match.
        assert!(store
            .match_title(Some("chrome.exe"), Some("Example Domain"), "en-US", &hc, &[])
            .is_none());
        fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn multi_pattern_title_match() {
        let t = std::env::temp_dir().join(format!("quicuts-multi-{}", std::process::id()));
        fs::create_dir_all(&t).unwrap();
        write(
            &t,
            "Acme.Docs.en-US.yml",
            "PackageName: Acme.Docs\nHost: browser\nTitleMatch:\n  - \"- Acme Docs\"\n  - \"| Acme Docs\"\nShortcuts:\n  - SectionName: S\n    Properties:\n      - Name: E\n        Shortcut:\n        - Keys:\n            - A\n",
        );
        let mut store = ManifestStore::new();
        store.load_dir(&t, SourceKind::Bundled);
        let hc = HostClasses::builtin();

        for title in ["Draft - Acme Docs", "Draft | Acme Docs"] {
            let hit = store.match_title(Some("chrome.exe"), Some(title), "en-US", &hc, &[]);
            assert_eq!(hit.unwrap().manifest.id, "Acme.Docs", "title {title:?}");
        }
        fs::remove_dir_all(&t).ok();
    }

    #[test]
    fn binding_matching_and_precedence() {
        let t = std::env::temp_dir().join(format!("quicuts-bind-{}", std::process::id()));
        fs::create_dir_all(&t).unwrap();
        write(&t, "Google.Gmail.en-US.yml", &mk_hosted("Google.Gmail", Some("- Gmail")));
        write(&t, "Acme.Notion.en-US.yml", &mk_hosted("Acme.Notion", None));
        write(&t, "Plain.App.en-US.yml", &mk("Plain.App", "plain.exe", false, ""));
        let mut store = ManifestStore::new();
        store.load_dir(&t, SourceKind::Bundled);
        let hc = HostClasses::builtin();
        let bind = |pattern: &str, id: &str| TitleBinding {
            pattern: pattern.into(),
            manifest_id: id.into(),
        };

        // A user binding matches where no manifest pattern does (the
        // Workspace-Gmail case: per-org signature).
        let ws_title = "Inbox (7) - m@carbonregister.co.uk - Carbon Register Mail";
        let bindings = [bind("Carbon Register Mail", "Google.Gmail")];
        let hit = store.match_title(Some("chrome.exe"), Some(ws_title), "en-US", &hc, &bindings);
        assert_eq!(hit.unwrap().manifest.id, "Google.Gmail");

        // A user binding beats a longer manifest pattern on the same title.
        let bindings = [bind("Inbox", "Acme.Notion")];
        let hit = store.match_title(
            Some("chrome.exe"),
            Some("Inbox - Gmail"),
            "en-US",
            &hc,
            &bindings,
        );
        assert_eq!(hit.unwrap().manifest.id, "Acme.Notion");

        // Among user bindings, the longest pattern wins.
        let bindings = [bind("Mail", "Acme.Notion"), bind("Register Mail", "Google.Gmail")];
        let hit = store.match_title(Some("chrome.exe"), Some(ws_title), "en-US", &hc, &bindings);
        assert_eq!(hit.unwrap().manifest.id, "Google.Gmail");

        // Bindings are host-gated like everything else.
        let bindings = [bind("Gmail", "Google.Gmail")];
        assert!(store
            .match_title(Some("notepad.exe"), Some("x - Gmail"), "en-US", &hc, &bindings)
            .is_none());

        // Dangling binding (unknown id) and a binding to a non-hosted
        // manifest are both inert.
        let bindings = [bind("Inbox", "Gone.App"), bind("Inbox", "Plain.App")];
        assert!(store
            .match_title(Some("chrome.exe"), Some("Inbox of things"), "en-US", &hc, &bindings)
            .is_none());

        // Empty/whitespace binding patterns never match.
        let bindings = [bind("  ", "Google.Gmail")];
        assert!(store
            .match_title(Some("chrome.exe"), Some("anything"), "en-US", &hc, &bindings)
            .is_none());
        fs::remove_dir_all(&t).ok();
    }

    /// A store with an exact match, a hosted collection, a *non*-background
    /// wildcard, and a background wildcard.
    fn fg_store(t: &Path) -> ManifestStore {
        fs::create_dir_all(t).unwrap();
        write(t, "Acme.App.en-US.yml", &mk("Acme.App", "app.exe", false, ""));
        write(t, "Acme.Star.en-US.yml", &mk("Acme.Star", "*", false, ""));
        write(t, "Acme.Bg.en-US.yml", &mk("Acme.Bg", "*", true, ""));
        write(t, "Acme.Web.en-US.yml", &mk_hosted("Acme.Web", Some("- Web")));
        let mut s = ManifestStore::new();
        s.load_dir(t, SourceKind::Bundled);
        s
    }

    fn fg_id(m: &[Matched<'_>], i: Option<usize>) -> Option<String> {
        i.map(|i| m[i].lm.manifest.id.clone())
    }

    /// An exact match leads; a hosted collection in the same rail must not
    /// steal the default, and a wildcard is the fallback when one exists.
    #[test]
    fn foreground_entry_prefers_exact_then_wildcard() {
        let t = std::env::temp_dir().join(format!("quicuts-fg1-{}", std::process::id()));
        let s = fg_store(&t);
        let hc = HostClasses::builtin();

        let m = s.match_foreground(Some("app.exe"), "en-US", &hc, |_| false);
        assert_eq!(fg_id(&m, foreground_entry(&m, None)).as_deref(), Some("Acme.App"));

        // A browser with no manifest of its own: the hosted collection is
        // present and ranked first, but the wildcard is the default.
        let m = s.match_foreground(Some("chrome.exe"), "en-US", &hc, |_| false);
        assert_eq!(m[0].kind, MatchKind::Hosted);
        assert_eq!(fg_id(&m, foreground_entry(&m, None)).as_deref(), Some("Acme.Star"));
        let _ = fs::remove_dir_all(&t);
    }

    /// Detection wins when it has a match; a stale one falls back to the
    /// rule rather than selecting nothing.
    #[test]
    fn foreground_entry_follows_title_detection() {
        let t = std::env::temp_dir().join(format!("quicuts-fg2-{}", std::process::id()));
        let s = fg_store(&t);
        let hc = HostClasses::builtin();
        let m = s.match_foreground(Some("chrome.exe"), "en-US", &hc, |_| false);
        assert_eq!(fg_id(&m, foreground_entry(&m, Some("Acme.Web"))).as_deref(), Some("Acme.Web"));
        assert_eq!(fg_id(&m, foreground_entry(&m, Some("Nope"))).as_deref(), Some("Acme.Star"));
        let _ = fs::remove_dir_all(&t);
    }

    /// **The regression guard, in the shape production actually produces.**
    ///
    /// Every bundled `"*"` manifest is also `BackgroundProcess: true`
    /// (`+WindowsNT.Shell`, `Apple.System`), and `match_foreground` tests
    /// background before wildcard — so in the real app the Wildcard group is
    /// empty and a browser with no manifest yields `[Hosted.., Background..]`.
    /// The old "first non-background" rule handed that user the hosted
    /// collection on every page. The answer must be `None` — the caller's
    /// unsupported-app placeholder.
    #[test]
    fn foreground_entry_never_defaults_to_a_hosted_collection() {
        let t = std::env::temp_dir().join(format!("quicuts-fg3-{}", std::process::id()));
        fs::create_dir_all(&t).unwrap();
        write(&t, "Acme.Shell.en-US.yml", &mk("Acme.Shell", "*", true, ""));
        write(&t, "Acme.Web.en-US.yml", &mk_hosted("Acme.Web", Some("- Web")));
        let mut s = ManifestStore::new();
        s.load_dir(&t, SourceKind::Bundled);
        let hc = HostClasses::builtin();

        let m = s.match_foreground(Some("firefox.exe"), "en-US", &hc, |_| true);
        assert_eq!(m[0].kind, MatchKind::Hosted);
        assert!(
            !m.iter().any(|x| x.kind == MatchKind::Wildcard),
            "production has no Wildcard-kind entry; the fixture must not invent one"
        );
        assert_eq!(foreground_entry(&m, None), None);
        // Detection still selects it when it genuinely matched.
        assert_eq!(fg_id(&m, foreground_entry(&m, Some("Acme.Web"))).as_deref(), Some("Acme.Web"));
        let _ = fs::remove_dir_all(&t);
    }
}
