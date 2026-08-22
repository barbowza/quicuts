//! The macOS manifest set in /manifests-mac must parse and match by bundle
//! id through the same platform-agnostic pipeline as the Windows set.

use std::path::PathBuf;

use quicuts_manifest::{HostClasses, ManifestStore, SourceKind};

fn store() -> ManifestStore {
    let mut s = ManifestStore::new();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests-mac");
    let (ok, failed) = s.load_dir(&dir, SourceKind::Bundled);
    assert_eq!(failed, 0, "some mac manifests failed to parse");
    assert!(ok >= 5, "expected the mac manifests, parsed {ok}");
    s
}

#[test]
fn all_mac_manifests_parse() {
    assert!(store().len() >= 5);
}

/// Evernote is the first Quicuts-maintained (non-Apple) mac manifest: it must
/// parse, match on its bundle id, and keep its ⌘-as-Win modifier mapping.
#[test]
fn evernote_mac_manifest_shape() {
    let s = store();
    let lm = s
        .get("Evernote.Evernote", "en-US")
        .expect("evernote mac manifest");
    assert_eq!(lm.manifest.window_filter, "com.evernote.Evernote");
    assert!(!lm.manifest.background_process);

    // The article's own section names, in its order.
    let names: Vec<_> = lm
        .manifest
        .sections
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "Global",
            "General",
            "Navigation",
            "View",
            "Note actions",
            "Editing",
            "Text formatting",
            "AI Assistant",
        ]
    );

    let section = |n: &str| {
        lm.manifest
            .sections
            .iter()
            .find(|s| s.name == n)
            .unwrap_or_else(|| panic!("section {n} missing"))
    };

    // ⌘N (New note) maps to Win, not Ctrl — the mac modifier mapping.
    let new_note = section("General")
        .entries
        .iter()
        .find(|e| e.name == "New note")
        .expect("New note");
    let combo = &new_note.combos[0];
    assert!(combo.win, "⌘ must map to Win on macOS");
    assert!(!combo.ctrl && !combo.alt && !combo.shift);

    // Mac-only rows the Windows manifest cannot have.
    for n in ["Hide Evernote", "Quit Evernote"] {
        assert!(
            section("General").entries.iter().any(|e| e.name == n),
            "{n} missing"
        );
    }

    // Foreground matching resolves the bundle id to this manifest first.
    let hc = HostClasses::builtin();
    let ids: Vec<String> = s
        .match_foreground(Some("com.evernote.Evernote"), "en-US", &hc, |_| false)
        .iter()
        .map(|m| m.lm.manifest.id.clone())
        .collect();
    assert_eq!(ids.first().map(String::as_str), Some("Evernote.Evernote"));
}

#[test]
fn bundle_id_matching_works_unchanged() {
    let s = store();
    let hc = HostClasses::builtin();
    let ids = |identity: &str| -> Vec<String> {
        s.match_foreground(Some(identity), "en-US", &hc, |_| false)
            .iter()
            .map(|m| m.lm.manifest.id.clone())
            .collect()
    };
    // Safari foreground: exact bundle-id match first, system manifest last.
    // Case-insensitive via the same normalize as .exe names.
    let safari = ids("com.apple.Safari");
    assert_eq!(safari.first().map(String::as_str), Some("Apple.Safari"));
    assert!(safari.contains(&"Apple.System".to_string()));
    // Unknown app still gets the wildcard system manifest.
    let unknown = ids("com.example.unknown");
    assert!(unknown.contains(&"Apple.System".to_string()));
    assert!(!unknown.contains(&"Apple.Safari".to_string()));
    // Finder and VS Code resolve to their own manifests.
    assert_eq!(ids("com.apple.finder").first().map(String::as_str), Some("Apple.Finder"));
    assert_eq!(
        ids("com.microsoft.VSCode").first().map(String::as_str),
        Some("Microsoft.VSCode")
    );
}

#[test]
fn system_manifest_is_wildcard_background() {
    let s = store();
    let sys = s.get("Apple.System", "en-US").expect("system manifest");
    assert_eq!(sys.manifest.window_filter, "*");
    assert!(sys.manifest.background_process);
    assert_eq!(sys.manifest.display_name(), "macOS");
}

/// Chrome is the mac set's first browser with a Quicuts-maintained manifest.
/// Its mac shortcuts genuinely differ from the Windows ones (⌥⌘I for
/// DevTools, ⌥⌘→ for the next tab), so this guards the mapping, not just
/// that the file parses.
#[test]
fn chrome_mac_manifest_shape() {
    let s = store();
    let lm = s
        .get("Google.Chrome", "en-US")
        .expect("chrome mac manifest");
    assert_eq!(lm.manifest.window_filter, "com.google.Chrome");
    assert!(!lm.manifest.background_process);

    // Section names mirror the Windows Chrome manifest.
    let names: Vec<_> = lm
        .manifest
        .sections
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "Tabs and windows",
            "Chrome features",
            "Address bar",
            "Web page"
        ]
    );

    let entry = |n: &str| {
        lm.manifest
            .sections
            .iter()
            .flat_map(|s| s.entries.iter())
            .find(|e| e.name == n)
            .unwrap_or_else(|| panic!("entry {n} missing"))
    };

    // ⌥⌘I, not Ctrl+Shift+J: the mac binding, not a translated Windows one.
    let devtools = &entry("Open Developer Tools").combos[0];
    assert!(devtools.win && devtools.alt);
    assert!(!devtools.ctrl && !devtools.shift);

    // "or" rows keep both alternatives on one entry.
    assert_eq!(
        entry("Open the previous page in your history").combos.len(),
        2
    );

    // cmd+R is absent from Google's Mac tables and added deliberately; it
    // must stay distinct from the ignore-cache variant (cmd+shift+R).
    let reload = &entry("Reload current page").combos[0];
    assert!(reload.win && !reload.shift && !reload.ctrl && !reload.alt);
    let hard = &entry("Reload, ignoring cached content").combos[0];
    assert!(hard.win && hard.shift);

    // Foreground matching resolves the bundle id to this manifest first.
    let hc = HostClasses::builtin();
    let ids: Vec<String> = s
        .match_foreground(Some("com.google.Chrome"), "en-US", &hc, |_| false)
        .iter()
        .map(|m| m.lm.manifest.id.clone())
        .collect();
    assert_eq!(ids.first().map(String::as_str), Some("Google.Chrome"));
}

/// iTerm2's manifest is read from the app's own menu bar rather than a doc
/// page, so this pins the parts that decoding could plausibly get wrong: the
/// cmd bit is inverted in AXMenuItemCmdModifiers (0x8 means *no* command),
/// and ctrl must stay distinct from cmd.
#[test]
fn iterm2_mac_manifest_shape() {
    let s = store();
    let lm = s
        .get("Googlecode.iTerm2", "en-US")
        .expect("iterm2 mac manifest");
    assert_eq!(lm.manifest.window_filter, "com.googlecode.iterm2");
    assert!(!lm.manifest.background_process);

    let names: Vec<_> = lm
        .manifest
        .sections
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "Shell",
            "Tabs",
            "Panes",
            "Broadcast input",
            "tmux",
            "Edit",
            "Find",
            "Marks and annotations",
            "View",
            "Session",
            "Window",
            "Application"
        ]
    );

    let entry = |n: &str| {
        lm.manifest
            .sections
            .iter()
            .flat_map(|s| s.entries.iter())
            .find(|e| e.name == n)
            .unwrap_or_else(|| panic!("entry {n} missing"))
    };

    // Plain cmd item: the inverted cmd bit decoded the right way round.
    let new_tab = &entry("New Tab").combos[0];
    assert!(new_tab.win);
    assert!(!new_tab.ctrl && !new_tab.shift && !new_tab.alt);

    // cmd+ctrl item: ⌃ must land on Ctrl, not be folded into Win.
    let divider = &entry("Move Divider Up").combos[0];
    assert!(divider.win && divider.ctrl);
    assert!(!divider.shift && !divider.alt);

    // fn has no PTSG flag, so it renders as its own leading keycap.
    let fullscreen = &entry("Toggle Full Screen").combos[0];
    assert!(!fullscreen.win && !fullscreen.ctrl && !fullscreen.shift && !fullscreen.alt);
    assert_eq!(fullscreen.keys.len(), 2);

    let hc = HostClasses::builtin();
    let ids: Vec<String> = s
        .match_foreground(Some("com.googlecode.iterm2"), "en-US", &hc, |_| false)
        .iter()
        .map(|m| m.lm.manifest.id.clone())
        .collect();
    assert_eq!(ids.first().map(String::as_str), Some("Googlecode.iTerm2"));
}
