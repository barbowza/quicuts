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

/// The mac set is keyed by bundle id; a `.exe` here is a copy/paste escape
/// from the Windows set.
fn check_not_exe(path: &std::path::Path, line: &str) {
    let v = line.trim_end().trim_matches('"').to_ascii_lowercase();
    assert!(!v.ends_with(".exe"), "{}: Windows-style {}", path.display(), line);
}

#[test]
fn all_mac_manifests_parse() {
    assert!(store().len() >= 5);
    // The mac set is keyed by bundle id, not exe name — a `.exe` filter in
    // here would be a copy/paste escape from the Windows set. Linted over
    // the files themselves; the store does not expose its whole contents.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests-mac");
    for f in std::fs::read_dir(dir).unwrap() {
        let path = f.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("yml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        // Covers both `WindowFilter: x` and the list form's `  - x` items.
        let mut in_filter = false;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with("WindowFilter:") {
                in_filter = true;
                check_not_exe(&path, t);
                continue;
            }
            if in_filter && t.starts_with("- ") {
                check_not_exe(&path, t);
                continue;
            }
            in_filter = false;
        }
    }
}

/// Evernote is the first Quicuts-maintained (non-Apple) mac manifest: it must
/// parse, match on its bundle id, and keep its ⌘-as-Win modifier mapping.
#[test]
fn evernote_mac_manifest_shape() {
    let s = store();
    let lm = s
        .get("Evernote.Evernote", "en-US")
        .expect("evernote mac manifest");
    assert_eq!(lm.manifest.window_filters, vec!["com.evernote.Evernote"]);
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
    assert!(sys.manifest.is_wildcard());
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
    assert_eq!(lm.manifest.window_filters, vec!["com.google.Chrome"]);
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
    assert_eq!(lm.manifest.window_filters, vec!["com.googlecode.iterm2"]);
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

/// The port of the two hosted collections (issue #19). Same ids and
/// `TitleMatch` patterns as the Windows files, so the engine, the settings
/// bindings UI, and every existing test above behave identically; only the
/// send chord differs.
#[test]
fn mac_hosted_collections_use_cmd_to_send() {
    let s = store();
    let send = |id: &str, entry: &str| {
        let lm = s.get(id, "en-US").unwrap_or_else(|| panic!("{id} missing from manifests-mac"));
        lm.manifest
            .sections
            .iter()
            .flat_map(|sec| &sec.entries)
            .find(|e| e.name == entry)
            .unwrap_or_else(|| panic!("{id}: no entry {entry:?}"))
            .combos[0]
            .clone()
    };
    // Cmd+Enter, not Ctrl+Enter. `Win` is PTSG's flag for Cmd on macOS.
    for (id, entry) in [("Google.Gmail", "Send"), ("Yahoo.YahooMail", "Send message")] {
        let c = send(id, entry);
        assert!(c.win, "{id}/{entry} should be Cmd-modified on macOS");
        assert!(!c.ctrl, "{id}/{entry} should not be Ctrl-modified on macOS");
    }
    // Gmail's other Ctrl binding moved too.
    let link = send("Google.Gmail", "Insert link");
    assert!(link.win && !link.ctrl);

    let gmail = s.get("Google.Gmail", "en-US").unwrap();
    assert_eq!(gmail.manifest.host.as_deref(), Some("browser"));
    assert_eq!(gmail.manifest.title_match, vec!["- Gmail"]);
    let icon = gmail.path.parent().unwrap().join(gmail.manifest.icon.as_deref().unwrap());
    assert!(icon.is_file(), "missing bundled icon {}", icon.display());
    assert_eq!(
        s.get("Yahoo.YahooMail", "en-US").unwrap().manifest.title_match,
        vec!["Yahoo Mail"]
    );
}

/// End-to-end for the whole issue: a macOS agent reports a *bundle id*, so
/// the browser host class has to admit it before a hosted collection can
/// ever reach the rail or a title match.
#[test]
fn mac_bundle_ids_drive_hosted_matching() {
    let s = store();
    let hc = HostClasses::builtin();
    let ids = |bundle: &str| -> Vec<String> {
        s.match_foreground(Some(bundle), "en-US", &hc, |_| false)
            .iter()
            .map(|m| m.lm.manifest.id.clone())
            .collect()
    };
    let pos = |id: &str, v: &[String]| v.iter().position(|x| x == id);

    // Safari: the host's own collection first, then both hosted ones.
    let safari = ids("com.apple.Safari");
    assert!(pos("Apple.Safari", &safari).unwrap() < pos("Google.Gmail", &safari).unwrap());
    assert!(pos("Yahoo.YahooMail", &safari).is_some());
    // Chrome and Firefox Developer Edition are browsers too...
    assert!(pos("Google.Gmail", &ids("com.google.Chrome")).is_some());
    assert!(pos("Google.Gmail", &ids("org.mozilla.firefoxdeveloperedition")).is_some());
    // ...iTerm is not, and shares a vendor prefix with nothing browser-like.
    assert!(pos("Google.Gmail", &ids("com.googlecode.iterm2")).is_none());

    // Title detection through a bundle id. Safari appends no browser suffix
    // to the window title (verified on real hardware), which the substring
    // patterns handle without a Safari-specific case.
    let hit = s.match_title(
        Some("com.apple.Safari"),
        Some("Inbox (3) - a@b.com - Gmail"),
        "en-US",
        &hc,
        &[],
    );
    assert_eq!(hit.unwrap().manifest.id, "Google.Gmail");
    // Chrome puts the profile name *after* the browser name; still a match.
    let hit = s.match_title(
        Some("com.google.Chrome"),
        Some("Yahoo Mail - Google Chrome \u{2013} Work"),
        "en-US",
        &hc,
        &[],
    );
    assert_eq!(hit.unwrap().manifest.id, "Yahoo.YahooMail");
    // A non-browser bundle id can never trigger a hosted collection.
    assert!(s
        .match_title(Some("com.apple.TextEdit"), Some("notes - Gmail"), "en-US", &hc, &[])
        .is_none());
}

/// Firefox is the first manifest to use a multi-value `WindowFilter`:
/// release, Developer Edition and Nightly are separate bundle ids with
/// identical shortcuts, and one manifest covers all three.
#[test]
fn firefox_mac_manifest_covers_every_edition() {
    let s = store();
    let hc = HostClasses::builtin();
    let lm = s.get("Mozilla.Firefox", "en-US").expect("firefox mac manifest");
    assert_eq!(lm.manifest.display_name(), "Firefox");
    assert_eq!(lm.manifest.window_filters.len(), 3);
    assert!(!lm.manifest.is_wildcard());

    let first = |identity: &str| -> Option<String> {
        s.match_foreground(Some(identity), "en-US", &hc, |_| false)
            .first()
            .map(|m| m.lm.manifest.id.clone())
    };
    // Every edition resolves to the one manifest...
    for id in [
        "org.mozilla.firefox",
        "org.mozilla.firefoxdeveloperedition",
        "org.mozilla.nightly",
    ] {
        assert_eq!(first(id).as_deref(), Some("Mozilla.Firefox"), "{id}");
    }
    // ...and it does not over-match a different Mozilla app.
    assert_ne!(first("org.mozilla.thunderbird").as_deref(), Some("Mozilla.Firefox"));

    // The regression that started this: Firefox is a browser, so hosted
    // collections join its rail — but Firefox itself now leads it, rather
    // than Gmail presenting as the foreground app.
    let rail = s.match_foreground(
        Some("org.mozilla.firefoxdeveloperedition"),
        "en-US",
        &hc,
        |_| false,
    );
    assert_eq!(rail[0].lm.manifest.id, "Mozilla.Firefox");
    assert_eq!(rail[0].kind, quicuts_manifest::MatchKind::Exact);
    assert!(rail.iter().any(|m| m.lm.manifest.id == "Google.Gmail"));
}
