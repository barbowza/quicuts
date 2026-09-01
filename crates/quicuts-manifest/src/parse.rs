//! YAML parsing + normalization of raw PTSG manifests into the typed model.
//! serde_yaml_ng is isolated here so the YAML backend can be swapped
//! (serde_norway / saphyr) without touching the rest of the engine.

use crate::keys::{normalize_token, TASKBAR_TOKEN};
use crate::schema::RawManifest;
use crate::{Entry, KeyCombo, Manifest, Section};

#[derive(Debug)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "manifest parse error: {}", self.0)
    }
}
impl std::error::Error for ParseError {}

/// Parse one manifest file's contents. `fallback_package_name` (usually the
/// filename stem) is used when the file omits `PackageName`.
pub fn parse_manifest(yaml: &str, fallback_package_name: &str) -> Result<Manifest, ParseError> {
    let raw: RawManifest =
        serde_yaml_ng::from_str(yaml).map_err(|e| ParseError(e.to_string()))?;
    Ok(normalize(raw, fallback_package_name))
}

fn normalize(raw: RawManifest, fallback_package_name: &str) -> Manifest {
    let sections = raw
        .shortcuts
        .unwrap_or_default()
        .into_iter()
        .map(|cat| {
            let name = cat.section_name.unwrap_or_default();
            let trimmed = name.trim();
            let inner = trimmed
                .strip_prefix('<')
                .and_then(|r| r.find('>').map(|i| &r[..i]));
            // "<TASKBAR1-9>" (optionally with trailing text) = taskbar section;
            // any other "<...>" name = meta section, hidden from the list.
            let is_taskbar = inner
                .map(|i| i.trim().eq_ignore_ascii_case(TASKBAR_TOKEN))
                .unwrap_or(false);
            let is_meta = inner.is_some() && !is_taskbar;
            let display_name = if is_taskbar {
                let rest = trimmed
                    .split_once('>')
                    .map(|(_, rest)| rest.trim())
                    .unwrap_or("");
                if rest.is_empty() { "Taskbar".to_string() } else { rest.to_string() }
            } else {
                trimmed.to_string()
            };
            Section {
                name: display_name,
                is_taskbar,
                is_meta,
                entries: cat
                    .properties
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|e| {
                        let name = e.name?;
                        Some(Entry {
                            name: name.trim().to_string(),
                            description: e
                                .description
                                .map(|d| d.trim().to_string())
                                .filter(|d| !d.is_empty()),
                            recommended: e.recommended.0,
                            combos: e
                                .shortcut
                                .unwrap_or_default()
                                .into_iter()
                                .map(|c| KeyCombo {
                                    win: c.win.0,
                                    ctrl: c.ctrl.0,
                                    shift: c.shift.0,
                                    alt: c.alt.0,
                                    keys: c
                                        .keys
                                        .unwrap_or_default()
                                        .iter()
                                        .map(normalize_token)
                                        .collect(),
                                })
                                .collect(),
                        })
                    })
                    .collect(),
            }
        })
        .collect();

    Manifest {
        id: raw
            .package_name
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| fallback_package_name.to_string()),
        name: raw.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
        window_filters: raw
            .window_filter
            .0
            .into_iter()
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty())
            .collect(),
        background_process: raw.background_process.0,
        host: raw
            .host
            .map(|h| h.trim().to_ascii_lowercase())
            .filter(|h| !h.is_empty()),
        title_match: raw
            .title_match
            .0
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
        icon: raw.icon.map(|i| i.trim().to_string()).filter(|i| !i.is_empty()),
        sections,
    }
}

/// Split `<PackageName>.<locale>.yml` into (package_name, locale).
/// PackageName itself contains dots ("Google.Chrome"), so the locale is the
/// final pre-extension segment only if it looks like a BCP-47 tag; otherwise
/// the whole stem is the package name and locale defaults to "en-US".
pub fn split_filename(file_name: &str) -> Option<(String, String)> {
    let stem = file_name
        .strip_suffix(".yml")
        .or_else(|| file_name.strip_suffix(".yaml"))?;
    if let Some((pkg, tail)) = stem.rsplit_once('.') {
        if looks_like_locale(tail) && !pkg.is_empty() {
            return Some((pkg.to_string(), tail.to_string()));
        }
    }
    Some((stem.to_string(), "en-US".to_string()))
}

fn looks_like_locale(s: &str) -> bool {
    let mut parts = s.split('-');
    let lang = match parts.next() {
        Some(l) => l,
        None => return false,
    };
    if !(2..=3).contains(&lang.len()) || !lang.chars().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    parts.all(|p| !p.is_empty() && p.len() <= 8 && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_split() {
        assert_eq!(
            split_filename("Google.Chrome.en-US.yml"),
            Some(("Google.Chrome".into(), "en-US".into()))
        );
        assert_eq!(
            split_filename("+WindowsNT.Shell.en-US.yml"),
            Some(("+WindowsNT.Shell".into(), "en-US".into()))
        );
        // No locale segment: whole stem is the package name.
        assert_eq!(
            split_filename("MyApp.yml"),
            Some(("MyApp".into(), "en-US".into()))
        );
        // "Chrome" is not a locale.
        assert_eq!(
            split_filename("Google.Chrome.yml"),
            Some(("Google.Chrome".into(), "en-US".into()))
        );
        assert_eq!(split_filename("notyaml.txt"), None);
    }

    #[test]
    fn taskbar_section_with_trailing_text() {
        let m = parse_manifest(
            "PackageName: X\nShortcuts:\n  - SectionName: <TASKBAR1-9>Taskbar Shortcuts\n    Properties:\n",
            "X",
        )
        .unwrap();
        assert!(m.sections[0].is_taskbar);
        assert!(!m.sections[0].is_meta);
        assert_eq!(m.sections[0].name, "Taskbar Shortcuts");
    }

    #[test]
    fn meta_section_skipped() {
        let m = parse_manifest(
            "PackageName: X\nShortcuts:\n  - SectionName: <Internal>\n    Properties:\n",
            "X",
        )
        .unwrap();
        assert!(m.sections[0].is_meta);
    }

    #[test]
    fn hosted_collection_fields() {
        let m = parse_manifest(
            "PackageName: Google.Gmail\nName: Gmail\nHost: Browser\nTitleMatch: \" - Gmail \"\nIcon: Google.Gmail.png\nShortcuts:\n",
            "Google.Gmail",
        )
        .unwrap();
        assert_eq!(m.host.as_deref(), Some("browser")); // lowercased
        assert_eq!(m.title_match, vec!["- Gmail"]); // trimmed
        assert_eq!(m.icon.as_deref(), Some("Google.Gmail.png"));
        assert!(m.window_filters.is_empty()); // Host manifests need no WindowFilter
    }

    #[test]
    fn title_match_accepts_a_list() {
        let m = parse_manifest(
            "PackageName: X\nHost: browser\nTitleMatch:\n  - \" - Gmail \"\n  - \"\"\n  - \"– Gmail\"\nShortcuts:\n",
            "X",
        )
        .unwrap();
        assert_eq!(m.title_match, vec!["- Gmail", "– Gmail"]); // trimmed, empties dropped
    }

    #[test]
    fn hosted_fields_absent_or_empty_are_none() {
        let m = parse_manifest("PackageName: X\nShortcuts:\n", "X").unwrap();
        assert_eq!(m.host, None);
        assert!(m.title_match.is_empty());
        assert_eq!(m.icon, None);

        let m = parse_manifest(
            "PackageName: X\nHost: \"  \"\nTitleMatch: \"\"\nIcon: \" \"\nShortcuts:\n",
            "X",
        )
        .unwrap();
        assert_eq!(m.host, None);
        assert!(m.title_match.is_empty());
        assert_eq!(m.icon, None);
    }

    #[test]
    fn tolerates_null_properties_and_missing_fields() {
        let m = parse_manifest(
            "PackageName: Microsoft.PowerToys\nName: PowerToys\nBackgroundProcess: True\nWindowFilter: \"powertoys.exe\"\nShortcuts:\n  - SectionName: General\n    Properties:\n",
            "Microsoft.PowerToys",
        )
        .unwrap();
        assert!(m.background_process);
        assert_eq!(m.sections.len(), 1);
        assert!(m.sections[0].entries.is_empty());
    }

    /// `WindowFilter` accepts one identity or several. The scalar form is
    /// what all 36 bundled PowerToys manifests use and must keep parsing
    /// byte-identically; the list form is how one manifest covers an app
    /// whose editions ship under different identities.
    #[test]
    fn window_filter_takes_a_string_or_a_list() {
        let scalar = parse_manifest(
            "PackageName: A\nWindowFilter: Notepad.exe\nShortcuts: []\n",
            "A.en-US.yml",
        )
        .unwrap();
        assert_eq!(scalar.window_filters, vec!["Notepad.exe"]);
        assert_eq!(scalar.primary_filter(), "Notepad.exe");
        assert!(scalar.matches_identity("notepad"));
        assert!(!scalar.is_wildcard());

        let list = parse_manifest(
            "PackageName: B\nWindowFilter:\n  - org.mozilla.firefox\n  - org.mozilla.firefoxdeveloperedition\nShortcuts: []\n",
            "B.en-US.yml",
        )
        .unwrap();
        assert_eq!(list.window_filters.len(), 2);
        // Either edition matches; the first is the one shown.
        assert!(list.matches_identity("org.mozilla.firefox"));
        assert!(list.matches_identity("org.mozilla.firefoxdeveloperedition"));
        assert!(list.matches_identity("ORG.MOZILLA.FIREFOX"));
        assert!(!list.matches_identity("org.mozilla.thunderbird"));
        assert_eq!(list.primary_filter(), "org.mozilla.firefox");

        // Wildcard still works, and empty/missing stays empty.
        let star = parse_manifest("PackageName: C\nWindowFilter: \"*\"\nShortcuts: []\n", "C.en-US.yml").unwrap();
        assert!(star.is_wildcard());
        let none = parse_manifest("PackageName: D\nShortcuts: []\n", "D.en-US.yml").unwrap();
        assert!(none.window_filters.is_empty());
        assert_eq!(none.primary_filter(), "");
    }
}
