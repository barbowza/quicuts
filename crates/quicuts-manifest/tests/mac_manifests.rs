//! The macOS manifest set in /manifests-mac must parse and match by bundle
//! id through the same platform-agnostic pipeline as the Windows set.

use std::path::PathBuf;

use quicuts_manifest::{HostClasses, ManifestStore, SourceKind};

fn store() -> ManifestStore {
    let mut s = ManifestStore::new();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests-mac");
    let (ok, failed) = s.load_dir(&dir, SourceKind::Bundled);
    assert_eq!(failed, 0, "some mac manifests failed to parse");
    assert!(ok >= 4, "expected the 4 mac manifests, parsed {ok}");
    s
}

#[test]
fn all_mac_manifests_parse() {
    assert!(store().len() >= 4);
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
