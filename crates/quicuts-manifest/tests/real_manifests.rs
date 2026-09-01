//! Compatibility suite: every real PTSG manifest bundled in /manifests must
//! parse, and known structural facts about specific files must hold.

use std::collections::HashSet;
use std::path::PathBuf;

use quicuts_manifest::{assemble, HostClasses, Key, ManifestStore, SourceKind};

fn manifests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests")
}

fn store() -> ManifestStore {
    let mut s = ManifestStore::new();
    let (ok, failed) = s.load_dir(&manifests_dir(), SourceKind::Bundled);
    assert_eq!(failed, 0, "some bundled manifests failed to parse");
    assert!(ok >= 36, "expected all 36 bundled manifests, parsed {ok}");
    s
}

#[test]
fn all_bundled_manifests_parse() {
    let s = store();
    assert!(s.len() >= 36);
}

#[test]
fn gmail_manifest_shape() {
    let s = store();
    let gmail = s.get("Google.Gmail", "en-US").expect("Gmail manifest");
    let m = &gmail.manifest;
    assert_eq!(m.display_name(), "Gmail");
    assert_eq!(m.host.as_deref(), Some("browser"));
    assert_eq!(m.title_match, vec!["- Gmail"]);
    assert_eq!(m.icon.as_deref(), Some("Google.Gmail.png"));
    assert_eq!(m.window_filter, "");
    assert!(!m.background_process);
    assert!(m.sections.len() >= 4);
    // The declared icon file ships next to the manifest.
    let icon = gmail.path.parent().unwrap().join(m.icon.as_deref().unwrap());
    assert!(icon.is_file(), "missing bundled icon {}", icon.display());
}

#[test]
fn gmail_joins_browser_rails_only() {
    let s = store();
    let hc = HostClasses::builtin();
    let ids = |exe: &str| -> Vec<String> {
        s.match_foreground(Some(exe), "en-US", &hc, |_| false)
            .iter()
            .map(|m| m.lm.manifest.id.clone())
            .collect()
    };
    // Chrome foreground: host first, then Gmail, Shell last.
    let chrome = ids("chrome.exe");
    let pos = |id: &str, v: &[String]| v.iter().position(|x| x == id);
    assert!(pos("Google.Chrome", &chrome).unwrap() < pos("Google.Gmail", &chrome).unwrap());
    assert!(pos("Google.Gmail", &chrome).unwrap() < pos("+WindowsNT.Shell", &chrome).unwrap());
    // Firefox gets Gmail too; a non-browser does not.
    assert!(pos("Google.Gmail", &ids("firefox.exe")).is_some());
    assert!(pos("Google.Gmail", &ids("Code.exe")).is_none());
}

#[test]
fn gmail_title_detection() {
    let s = store();
    let hc = HostClasses::builtin();
    let hit = s.match_title(
        Some("firefox.exe"),
        Some("Inbox (3) - a@b.com - Gmail — Mozilla Firefox"),
        "en-US",
        &hc,
        &[],
    );
    assert_eq!(hit.unwrap().manifest.id, "Google.Gmail");
    assert!(s
        .match_title(Some("notepad.exe"), Some("notes - Gmail"), "en-US", &hc, &[])
        .is_none());
}

#[test]
fn workspace_gmail_via_user_binding() {
    // The motivating case for signature bindings: Workspace Gmail's title
    // carries the org name, not "Gmail" — only a user binding can match it.
    let s = store();
    let hc = HostClasses::builtin();
    let title = "Inbox (7) - michael@example.co.uk - Example Corp Mail";
    assert!(s.match_title(Some("chrome.exe"), Some(title), "en-US", &hc, &[]).is_none());
    let bindings = [quicuts_manifest::TitleBinding {
        pattern: "Example Corp Mail".into(),
        manifest_id: "Google.Gmail".into(),
    }];
    let hit = s.match_title(Some("chrome.exe"), Some(title), "en-US", &hc, &bindings);
    assert_eq!(hit.unwrap().manifest.id, "Google.Gmail");
}

#[test]
fn yahoo_mail_hosted_collection() {
    // Authored by us (ADR 0003), icon-less: the rail falls back to a letter
    // tile, so no `Icon:` file has to ship alongside it.
    let s = store();
    let hc = HostClasses::builtin();
    let yahoo = s.get("Yahoo.YahooMail", "en-US").expect("Yahoo Mail manifest");
    let m = &yahoo.manifest;
    assert_eq!(m.display_name(), "Yahoo Mail");
    assert_eq!(m.host.as_deref(), Some("browser"));
    assert_eq!(m.title_match, vec!["Yahoo Mail"]);
    assert_eq!(m.icon, None);
    assert_eq!(m.window_filter, "");
    assert!(!m.background_process);
    assert_eq!(m.sections.len(), 4);

    // Joins any browser rail behind the host, never a non-browser's.
    let ids = |exe: &str| -> Vec<String> {
        s.match_foreground(Some(exe), "en-US", &hc, |_| false)
            .iter()
            .map(|m| m.lm.manifest.id.clone())
            .collect()
    };
    let firefox = ids("firefox.exe");
    let pos = |id: &str, v: &[String]| v.iter().position(|x| x == id);
    assert!(pos("Mozilla.Firefox", &firefox).unwrap() < pos("Yahoo.YahooMail", &firefox).unwrap());
    assert!(pos("Yahoo.YahooMail", &ids("chrome.exe")).is_some());
    assert!(pos("Yahoo.YahooMail", &ids("Code.exe")).is_none());

    // Title detection, and the guard against a non-browser faking the title.
    let hit = s.match_title(
        Some("firefox.exe"),
        Some("Yahoo Mail \u{2014} Mozilla Firefox"),
        "en-US",
        &hc,
        &[],
    );
    assert_eq!(hit.unwrap().manifest.id, "Yahoo.YahooMail");
    assert!(s
        .match_title(Some("notepad.exe"), Some("notes - Yahoo Mail"), "en-US", &hc, &[])
        .is_none());
}

#[test]
fn ptsg_manifests_have_no_hosted_fields() {
    // The Quicuts schema extensions must not misfire on stock PTSG files.
    let s = store();
    let hc = HostClasses::builtin();
    for m in s.match_foreground(Some("chrome.exe"), "en-US", &hc, |_| true) {
        if m.lm.manifest.id.starts_with("Google.Chrome")
            || m.lm.manifest.id.starts_with('+')
            || m.lm.manifest.id.starts_with("Microsoft.")
        {
            assert_eq!(m.lm.manifest.host, None, "{}", m.lm.manifest.id);
            assert!(m.lm.manifest.title_match.is_empty(), "{}", m.lm.manifest.id);
            assert_eq!(m.lm.manifest.icon, None, "{}", m.lm.manifest.id);
        }
    }
}

#[test]
fn shell_manifest_specialcases() {
    let s = store();
    let shell = s.get("+WindowsNT.Shell", "en-US").expect("Shell manifest");
    let m = &shell.manifest;
    assert_eq!(m.window_filter, "*");
    assert!(m.background_process);
    // Shell omits Name; display falls back to "Windows" like PTSG.
    assert_eq!(m.display_name(), "Windows");
    assert!(m.has_taskbar_section(), "Shell should have the <TASKBAR1-9> section");
}

#[test]
fn chrome_manifest_shape() {
    let s = store();
    let chrome = s.get("Google.Chrome", "en-US").expect("Chrome manifest");
    let m = &chrome.manifest;
    assert_eq!(m.display_name(), "Google Chrome");
    assert_eq!(m.window_filter, "chrome.exe");
    assert!(!m.background_process);
    assert!(!m.sections.is_empty());
    let first = &m.sections[0];
    assert_eq!(first.name, "Tabs and windows");
    let new_window = &first.entries[0];
    assert_eq!(new_window.name, "New window");
    assert!(new_window.recommended);
    let combo = &new_window.combos[0];
    assert!(combo.ctrl && !combo.win && !combo.shift && !combo.alt);
    assert_eq!(combo.keys, vec![Key::Literal("N".into())]);
}

#[test]
fn powertoys_manifest_empty_properties_tolerated() {
    let s = store();
    let pt = s.get("Microsoft.PowerToys", "en-US").expect("PowerToys manifest");
    let m = &pt.manifest;
    assert!(m.background_process, "BackgroundProcess: True (capital) must parse");
    // Its Properties list is only comments -> zero entries, but the section exists.
    assert!(m.sections.iter().all(|sec| sec.entries.is_empty()));
}

#[test]
fn every_manifest_assembles() {
    let s = store();
    let empty = HashSet::new();
    let hc = HostClasses::builtin();
    for exe in ["chrome.exe", "Code.exe", "firefox.exe", "unknown.exe"] {
        for m in s.match_foreground(Some(exe), "en-US", &hc, |_| false) {
            let page = assemble(&m.lm.manifest, &empty, &Default::default());
            assert_eq!(page.sections[0].title, "Pinned");
            // No meta sections leak into pages.
            assert!(page.sections.iter().all(|sec| !sec.title.starts_with('<')));
        }
    }
}

#[test]
fn angle_tokens_all_recognized_or_literal() {
    // Sweep every key of every manifest: normalization must never panic and
    // every token must land in a typed variant.
    let s = store();
    let mut glyphs = 0;
    let mut vks = 0;
    let hc = HostClasses::builtin();
    for exe in [None, Some("chrome.exe")] {
        for m in s.match_foreground(exe, "en-US", &hc, |_| true) {
            for sec in &m.lm.manifest.sections {
                for e in &sec.entries {
                    for c in &e.combos {
                        for k in &c.keys {
                            match k {
                                Key::Glyph(_) => glyphs += 1,
                                Key::Vk(_) => vks += 1,
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(glyphs > 0, "expected glyph tokens in Shell manifest");
    let _ = vks;
}
