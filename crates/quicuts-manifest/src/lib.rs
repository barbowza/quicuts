//! PTSG-compatible shortcut-manifest engine.
//!
//! Platform-free by design: parsing, indexing, foreground matching, and page
//! assembly all run anywhere (unit-tested on the Linux host toolchain).

pub mod custom;
pub mod host;
pub mod icon;
pub mod keys;
pub mod parse;
pub mod schema;
mod store;

pub use custom::{AppCustomizations, CustomBinding, EntryCustomization};
pub use host::{HostClasses, BROWSER_CLASS};
pub use keys::{GlyphToken, Key};
pub use parse::{parse_manifest, split_filename, ParseError};
pub use store::{LoadedManifest, ManifestStore, MatchKind, Matched, SourceKind};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyCombo {
    pub win: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub keys: Vec<Key>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub description: Option<String>,
    pub recommended: bool,
    /// Alternative key combos for the same action.
    pub combos: Vec<KeyCombo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub name: String,
    /// `<TASKBAR1-9>` section: shown last and arms the badge window.
    pub is_taskbar: bool,
    /// `<...>`-named section: hidden from the normal list.
    pub is_meta: bool,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// PTSG PackageName, e.g. "Google.Chrome" or "+WindowsNT.Shell".
    pub id: String,
    /// Display name; PTSG special-cases a missing Name on the Shell manifest
    /// to "Windows" — the caller decides the fallback label.
    pub name: Option<String>,
    /// Identities to match: exe names on Windows ("Notepad.exe",
    /// case-insensitive, ".exe" optional), bundle ids on macOS, or the
    /// single entry "*" for all windows. Usually one; more than one covers
    /// an app whose editions ship under different identities. Empty when
    /// `host` is set (a hosted collection matches only via its class).
    pub window_filters: Vec<String>,
    /// true: show whenever the process runs (with "*": always shown).
    pub background_process: bool,
    /// Host class for a hosted collection ("browser"), lowercased. A hosted
    /// manifest matches only via its host class, never via `window_filter`.
    pub host: Option<String>,
    /// Case-insensitive substrings matched against the foreground window
    /// title to auto-detect the hosted app (experimental title detection).
    /// Empty = no title patterns. All entries are trimmed and non-empty.
    pub title_match: Vec<String>,
    /// Image filename next to the manifest, used as the rail icon.
    pub icon: Option<String>,
    pub sections: Vec<Section>,
}

impl Manifest {
    /// The identity to show and to look a live process up by, when exactly
    /// one is needed (rail icons, `BackgroundProcess` hints). Empty string
    /// for a hosted collection, which has no identity of its own.
    pub fn primary_filter(&self) -> &str {
        self.window_filters.first().map(String::as_str).unwrap_or("")
    }

    /// The "*" filter: matches every window.
    pub fn is_wildcard(&self) -> bool {
        self.window_filters.iter().any(|f| f == "*")
    }

    /// Does this manifest claim `identity`? Both sides go through
    /// `normalize_exe`, so an exe name and a bundle id are compared by the
    /// same rule and the caller may pass either form.
    pub fn matches_identity(&self, identity: &str) -> bool {
        let want = normalize_exe(identity);
        self.window_filters.iter().any(|f| normalize_exe(f) == want)
    }

    pub fn display_name(&self) -> &str {
        match &self.name {
            Some(n) => n,
            None if self.id == "+WindowsNT.Shell" => "Windows",
            None => &self.id,
        }
    }

    pub fn has_taskbar_section(&self) -> bool {
        self.sections.iter().any(|s| s.is_taskbar)
    }
}

/// A user-captured title signature bound to a hosted collection: when
/// `pattern` (case-insensitive substring, same semantics as `TitleMatch`)
/// hits the foreground title of a host-class window, the collection named by
/// `manifest_id` auto-selects. User bindings always beat manifest
/// `TitleMatch` patterns. Persisted in settings, not in manifest files, so
/// bindings survive manifest updates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleBinding {
    pub pattern: String,
    pub manifest_id: String,
}

/// Normalize an exe identifier for matching: lowercase, strip one trailing
/// ".exe", basename only.
pub fn normalize_exe(name: &str) -> String {
    let base = name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    base.strip_suffix(".exe").unwrap_or(&base).to_string()
}

/// Stable identity of an entry for pinning: survives reordering, invalidates
/// when the shortcut itself changes. FNV-1a over section, name, and combos.
pub fn entry_key(section_name: &str, entry: &Entry) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut h = OFFSET;
    let mut feed = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(PRIME);
        }
        h ^= 0x1f;
        h = h.wrapping_mul(PRIME);
    };
    feed(section_name.as_bytes());
    feed(entry.name.as_bytes());
    for c in &entry.combos {
        feed(&[c.win as u8, c.ctrl as u8, c.shift as u8, c.alt as u8]);
        for k in &c.keys {
            feed(serde_json::to_string(k).unwrap_or_default().as_bytes());
        }
    }
    format!("{h:016x}")
}

/// A fully assembled, render-ready page for one app, in PTSG order:
/// Pinned, Recommended, categories in file order, Taskbar last.
#[derive(Debug, Clone, Serialize)]
pub struct AssembledPage {
    pub manifest_id: String,
    pub display_name: String,
    pub sections: Vec<AssembledSection>,
    pub has_taskbar: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssembledSection {
    pub title: String,
    #[serde(rename = "kind")]
    pub kind: SectionKind,
    pub entries: Vec<AssembledEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Pinned,
    Recommended,
    Category,
    Taskbar,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssembledEntry {
    /// Stable pin key (also the row identity for the UI).
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub pinned: bool,
    pub combos: Vec<KeyCombo>,
    /// User-added bindings from the app's customization file.
    #[serde(rename = "customCombos")]
    pub custom_combos: Vec<CustomBinding>,
    /// Parallel to `combos`: chord is part of a default binding the user
    /// marked as reassigned (no longer available).
    pub redefined: Vec<bool>,
}

/// Assemble the view for one manifest. `pinned` holds this manifest's pinned
/// entry keys; unknown keys are ignored (pins referencing vanished entries
/// drop silently, as in PTSG). `customs` layers the user's per-app shortcut
/// customizations onto matching entries (matched by shortcut name).
pub fn assemble(
    manifest: &Manifest,
    pinned: &std::collections::HashSet<String>,
    customs: &AppCustomizations,
) -> AssembledPage {
    let mut pinned_rows = Vec::new();
    let mut recommended_rows = Vec::new();
    let mut category_sections = Vec::new();
    let mut taskbar_section: Option<AssembledSection> = None;

    for section in &manifest.sections {
        if section.is_meta {
            continue;
        }
        let rows: Vec<AssembledEntry> = section
            .entries
            .iter()
            .map(|e| {
                let key = entry_key(&section.name, e);
                let is_pinned = pinned.contains(&key);
                let cust = customs.get(&e.name);
                AssembledEntry {
                    key,
                    name: e.name.clone(),
                    description: e.description.clone(),
                    pinned: is_pinned,
                    combos: e.combos.clone(),
                    custom_combos: custom::custom_bindings(cust),
                    redefined: custom::mark_redefined(
                        &e.combos,
                        cust.map(|c| c.redefined.as_slice()).unwrap_or(&[]),
                    ),
                }
            })
            .collect();

        for row in &rows {
            if row.pinned {
                pinned_rows.push(row.clone());
            }
        }
        for (row, entry) in rows.iter().zip(&section.entries) {
            if entry.recommended {
                recommended_rows.push(row.clone());
            }
        }

        let assembled = AssembledSection {
            title: section.name.clone(),
            kind: if section.is_taskbar { SectionKind::Taskbar } else { SectionKind::Category },
            entries: rows,
        };
        if section.is_taskbar {
            taskbar_section = Some(assembled);
        } else {
            category_sections.push(assembled);
        }
    }

    let mut sections = Vec::new();
    // Pinned always present (empty placeholder mirrors PTSG).
    sections.push(AssembledSection {
        title: "Pinned".into(),
        kind: SectionKind::Pinned,
        entries: pinned_rows,
    });
    if !recommended_rows.is_empty() {
        sections.push(AssembledSection {
            title: "Recommended".into(),
            kind: SectionKind::Recommended,
            entries: recommended_rows,
        });
    }
    sections.extend(category_sections);
    let has_taskbar = taskbar_section.is_some();
    sections.extend(taskbar_section);

    AssembledPage {
        manifest_id: manifest.id.clone(),
        display_name: manifest.display_name().to_string(),
        sections,
        has_taskbar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn entry(name: &str, recommended: bool) -> Entry {
        Entry {
            name: name.into(),
            description: None,
            recommended,
            combos: vec![KeyCombo {
                win: false,
                ctrl: true,
                shift: false,
                alt: false,
                keys: vec![Key::Literal("A".into())],
            }],
        }
    }

    #[test]
    fn normalize_exe_rules() {
        assert_eq!(normalize_exe("Notepad.exe"), "notepad");
        assert_eq!(normalize_exe("CHROME.EXE"), "chrome");
        assert_eq!(normalize_exe(r"C:\Program Files\App\app.exe"), "app");
        assert_eq!(normalize_exe("bash"), "bash");
    }

    #[test]
    fn entry_key_stability() {
        let a = entry("New tab", true);
        let k1 = entry_key("File", &a);
        assert_eq!(k1, entry_key("File", &a.clone()));
        // Changing the combo changes the key.
        let mut b = a.clone();
        b.combos[0].shift = true;
        assert_ne!(k1, entry_key("File", &b));
        // Changing the section changes the key.
        assert_ne!(k1, entry_key("Edit", &a));
    }

    #[test]
    fn assemble_order_and_sections() {
        let m = Manifest {
            id: "Test.App".into(),
            name: Some("Test".into()),
            window_filters: vec!["test.exe".into()],
            background_process: false,
            host: None,
            title_match: Vec::new(),
            icon: None,
            sections: vec![
                Section {
                    name: "Meta".into(),
                    is_taskbar: false,
                    is_meta: true,
                    entries: vec![entry("hidden", false)],
                },
                Section {
                    name: "Taskbar Shortcuts".into(),
                    is_taskbar: true,
                    is_meta: false,
                    entries: vec![entry("Open app N", false)],
                },
                Section {
                    name: "General".into(),
                    is_taskbar: false,
                    is_meta: false,
                    entries: vec![entry("Do thing", true), entry("Other", false)],
                },
            ],
        };
        let pinned: HashSet<String> =
            [entry_key("General", &entry("Other", false))].into_iter().collect();
        let page = assemble(&m, &pinned, &AppCustomizations::default());
        let kinds: Vec<SectionKind> = page.sections.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![SectionKind::Pinned, SectionKind::Recommended, SectionKind::Category, SectionKind::Taskbar]
        );
        assert_eq!(page.sections[0].entries.len(), 1); // pinned "Other"
        assert_eq!(page.sections[1].entries[0].name, "Do thing");
        assert!(page.has_taskbar);
        // Meta section absent.
        assert!(page.sections.iter().all(|s| s.title != "Meta"));
    }
}
