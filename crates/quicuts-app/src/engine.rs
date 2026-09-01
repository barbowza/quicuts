//! Wraps the manifest store and turns a foreground app into the render-ready
//! `OverlayState` view model the frontend consumes.

use std::collections::HashMap;
use std::path::PathBuf;

use quicuts_manifest::{assemble, AssembledSection, HostClasses, ManifestStore, SourceKind};
use quicuts_proto::ForegroundInfo;
use serde::Serialize;

use crate::icons;
use crate::pinned::PinStore;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RailApp {
    pub manifest_id: String,
    pub display_name: String,
    pub icon_url: Option<String>,
    pub is_foreground: bool,
    /// Placeholder for a foreground app with no collection (ADR 0004):
    /// rendered dimmed, not pinnable, page is empty.
    pub unsupported: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayStatePayload {
    pub platform: &'static str,
    pub apps: Vec<RailApp>,
    pub pages: HashMap<String, Vec<AssembledSection>>,
    pub selected: Option<String>,
    /// Rail app pinned by the user (stays listed + selected), if any.
    pub pinned_app: Option<String>,
    /// True if the selected page has a taskbar section (arms badges).
    pub has_taskbar: bool,
    /// Combo visibility: "default" | "custom" | "all" | "customElseDefault".
    pub combo_display_mode: String,
    /// Panel activation config, so the Help view can render the real
    /// hold/chord bindings instead of hardcoded defaults.
    pub hold_enabled: bool,
    pub chord_enabled: bool,
    pub chord: quicuts_proto::ChordSpec,
    /// Chord that toggles the settings window while the panel has focus;
    /// matched by the overlay webview itself, never by the agent hook.
    pub settings_chord_enabled: bool,
    pub settings_chord: quicuts_proto::ChordSpec,
}

pub struct Engine {
    store: ManifestStore,
    locale: String,
    icon_cache: HashMap<String, Option<String>>,
    /// exe path -> friendly name (version-info FileDescription).
    name_cache: HashMap<String, String>,
    bundled_dir: PathBuf,
    user_dir: PathBuf,
}

const PLATFORM: &str = if cfg!(target_os = "macos") { "macos" } else { "windows" };

/// Which rail entry presents as "the foreground app" — the one whose page
/// is shown by default and whose name labels the panel.
///
/// 1. The title-matched hosted collection, when detection has one. That is
///    the whole point of ADR 0003: while you are in the Gmail tab, Gmail
///    *is* the app you are in.
/// 2. Otherwise the first **exact or wildcard** match — never a hosted one.
///
/// Rule 2's exclusion is the subtle half. `match_foreground` orders groups
/// exact → hosted → wildcard → background, so "first non-background" picks
/// a hosted collection whenever the host browser has no manifest of its
/// own. On macOS that is the common case (`manifests-mac/` ships Safari and
/// Chrome but not Firefox), and the symptom is Gmail's shortcuts presenting
/// as the foreground app on a Firefox new-tab page, with no way to tell
/// from the panel that nothing matched. ADR 0003 says the *host* page stays
/// selected by default; when the host has no page, the honest answer is the
/// system-wide wildcard — or the unsupported-app placeholder — not a web
/// app the user may not even have open.
///
/// The hosted collections still appear in the rail and stay selectable by
/// hand; this only decides what is selected *for* the user.
fn foreground_entry(
    entries: &[(quicuts_manifest::MatchKind, &str)],
    title_matched: Option<&str>,
) -> Option<usize> {
    use quicuts_manifest::MatchKind;
    title_matched
        .and_then(|t| {
            entries.iter().position(|(k, id)| *k == MatchKind::Hosted && *id == t)
        })
        .or_else(|| {
            entries
                .iter()
                .position(|(k, _)| matches!(k, MatchKind::Exact | MatchKind::Wildcard))
        })
}

impl Engine {
    pub fn new(bundled_dir: PathBuf, user_dir: PathBuf) -> Self {
        let mut e = Engine {
            store: ManifestStore::new(),
            locale: "en-US".into(),
            icon_cache: HashMap::new(),
            name_cache: HashMap::new(),
            bundled_dir,
            user_dir,
        };
        e.reload();
        e
    }

    /// (Re)load all manifest sources in priority order.
    pub fn reload(&mut self) {
        self.store.clear();
        let (ok, failed) = self.store.load_dir(&self.bundled_dir, SourceKind::Bundled);
        log::info!("loaded {ok} bundled manifests ({failed} failed)");
        if let Some(ptsg) = ptsg_runtime_dir() {
            self.store.load_dir(&ptsg, SourceKind::PtsgRuntime);
        }
        self.store.load_dir(&self.user_dir, SourceKind::User);
    }

    fn icon_for(&mut self, exe_path: Option<&str>) -> Option<String> {
        let path = exe_path?;
        if let Some(cached) = self.icon_cache.get(path) {
            return cached.clone();
        }
        let uri = icons::icon_data_uri(path);
        self.icon_cache.insert(path.to_string(), uri.clone());
        uri
    }

    /// Hosted collection auto-detected from the foreground window title
    /// (experimental title detection), if any. `bindings` are the user's
    /// captured signatures; they beat manifest `TitleMatch` patterns.
    pub fn title_match(
        &self,
        fg: &ForegroundInfo,
        host_classes: &HostClasses,
        bindings: &[quicuts_manifest::TitleBinding],
    ) -> Option<String> {
        self.store
            .match_title(
                fg.exe_name.as_deref(),
                fg.title.as_deref(),
                &self.locale,
                host_classes,
                bindings,
            )
            .map(|lm| lm.manifest.id.clone())
    }

    /// Installed hosted collections as (manifest id, display name, own
    /// TitleMatch patterns) — the valid targets for a signature binding.
    pub fn hosted_manifests(&self) -> Vec<(String, String, Vec<String>)> {
        self.store
            .hosted(&self.locale)
            .into_iter()
            .map(|lm| {
                (
                    lm.manifest.id.clone(),
                    lm.manifest.display_name().to_string(),
                    lm.manifest.title_match.clone(),
                )
            })
            .collect()
    }

    /// Friendly name for a foreground exe, cached by path (the version-info
    /// read hits disk and build_state runs on every foreground change).
    fn name_for(&mut self, exe_path: Option<&str>, exe_name: &str) -> String {
        let Some(path) = exe_path else {
            return crate::appname::display_name(None, exe_name);
        };
        if let Some(cached) = self.name_cache.get(path) {
            return cached.clone();
        }
        let name = crate::appname::display_name(Some(path), exe_name);
        self.name_cache.insert(path.to_string(), name.clone());
        name
    }

    /// Data URI for a manifest-declared icon file, cached by path.
    fn file_icon_for(&mut self, path: Option<&std::path::Path>) -> Option<String> {
        let path = path?;
        let key = path.to_string_lossy().into_owned();
        if let Some(cached) = self.icon_cache.get(&key) {
            return cached.clone();
        }
        let uri = icons::file_data_uri(path);
        self.icon_cache.insert(key, uri.clone());
        uri
    }

    /// Assemble the full overlay view for a given foreground window.
    /// `title_matched` names the hosted collection currently detected via
    /// TitleMatch (it presents as the foreground rail app).
    #[allow(clippy::too_many_arguments)]
    pub fn build_state(
        &mut self,
        foreground: Option<&ForegroundInfo>,
        pins: &PinStore,
        customs: &crate::customizations::CustomStore,
        selected: Option<String>,
        pinned: Option<&str>,
        title_matched: Option<&str>,
        host_classes: &HostClasses,
        combo_display_mode: String,
        activation: crate::settings::Activation,
    ) -> OverlayStatePayload {
        let fg_exe = foreground.and_then(|f| f.exe_name.as_deref());
        let fg_exe_path = foreground.and_then(|f| f.exe_path.clone());

        // Live processes, so BackgroundProcess manifests (PowerToys, ...)
        // join the rail while their app runs.
        let running = crate::procs::running();

        // Snapshot matches to drop the store borrow before we assemble
        // pages / touch the icon cache.
        struct Snap {
            id: String,
            name: String,
            kind: quicuts_manifest::MatchKind,
            is_fg: bool,
            icon_file: Option<PathBuf>,
            exe_hint: Option<String>,
        }
        let mut matched: Vec<Snap> = self
            .store
            .match_foreground(fg_exe, &self.locale, host_classes, |exe| {
                running.contains_key(exe)
            })
            .into_iter()
            .map(|m| {
                let lm = m.lm;
                // A background app's icon comes from its live process.
                let filter = lm.manifest.primary_filter();
                let exe_hint = if lm.manifest.background_process && filter != "*" {
                    running
                        .get(&quicuts_manifest::normalize_exe(filter))
                        .and_then(|pid| crate::procs::exe_path(*pid))
                } else {
                    None
                };
                Snap {
                    id: lm.manifest.id.clone(),
                    name: lm.manifest.display_name().to_string(),
                    kind: m.kind,
                    is_fg: false,
                    icon_file: manifest_icon_path(lm),
                    exe_hint,
                }
            })
            .collect();

        let fg_index = foreground_entry(
            &matched.iter().map(|s| (s.kind, s.id.as_str())).collect::<Vec<_>>(),
            title_matched,
        );
        if let Some(i) = fg_index {
            matched[i].is_fg = true;
        }

        let mut apps = Vec::new();
        let mut pages = HashMap::new();
        for Snap { id, name, kind, is_fg, icon_file, exe_hint } in &matched {
            // Manifest-declared icon wins. The foreground entry falls back
            // to the foreground exe's icon — except hosted collections,
            // which must not wear their host browser's icon (the UI letter
            // tile takes over instead). Background apps use their live
            // process icon.
            let icon_url = self.file_icon_for(icon_file.as_deref()).or_else(|| {
                if *is_fg && *kind != quicuts_manifest::MatchKind::Hosted {
                    self.icon_for(fg_exe_path.as_deref())
                } else {
                    self.icon_for(exe_hint.as_deref())
                }
            });
            apps.push(RailApp {
                manifest_id: id.clone(),
                display_name: name.clone(),
                icon_url,
                is_foreground: *is_fg,
                unsupported: false,
            });
            let sections = self
                .store
                .get(id, &self.locale)
                .map(|lm| assemble(&lm.manifest, &pins.get(id), &customs.get(id)).sections)
                .unwrap_or_default();
            pages.insert(id.clone(), sections);
        }

        // Unsupported foreground (ADR 0004): a known exe with no collection
        // marked foreground gets a dimmed placeholder tile with the real
        // icon/name and an empty page. Unknown exe (desktop, secure
        // windows) keeps the plain fallback.
        if fg_index.is_none() {
            if let Some(exe) = fg_exe {
                let id = format!("unsupported:{}", quicuts_manifest::normalize_exe(exe));
                apps.insert(
                    0,
                    RailApp {
                        manifest_id: id.clone(),
                        display_name: self.name_for(fg_exe_path.as_deref(), exe),
                        icon_url: self.icon_for(fg_exe_path.as_deref()),
                        is_foreground: true,
                        unsupported: true,
                    },
                );
                pages.insert(id, Vec::new());
            }
        }

        // A pinned app leads the rail even when the foreground is elsewhere.
        if let Some(pin) = pinned {
            if let Some(pos) = apps.iter().position(|a| a.manifest_id == pin) {
                let a = apps.remove(pos);
                apps.insert(0, a);
            } else {
                // Not matched by the current foreground: pull it from the
                // store directly (manifest icon, else its live process).
                let info = self.store.get(pin, &self.locale).map(|lm| {
                    (
                        lm.manifest.display_name().to_string(),
                        lm.manifest.primary_filter().to_string(),
                        manifest_icon_path(lm),
                        assemble(&lm.manifest, &pins.get(pin), &customs.get(pin)).sections,
                    )
                });
                if let Some((name, filter, icon_file, sections)) = info {
                    let icon_path = running
                        .get(&quicuts_manifest::normalize_exe(&filter))
                        .and_then(|pid| crate::procs::exe_path(*pid));
                    let icon_url = self
                        .file_icon_for(icon_file.as_deref())
                        .or_else(|| self.icon_for(icon_path.as_deref()));
                    apps.insert(
                        0,
                        RailApp {
                            manifest_id: pin.to_string(),
                            display_name: name,
                            icon_url,
                            is_foreground: false,
                            unsupported: false,
                        },
                    );
                    pages.insert(pin.to_string(), sections);
                }
            }
        }

        let selected = selected
            .filter(|s| pages.contains_key(s))
            .or_else(|| pinned.filter(|p| pages.contains_key(*p)).map(str::to_string))
            .or_else(|| apps.first().map(|a| a.manifest_id.clone()));
        let has_taskbar = selected
            .as_ref()
            .and_then(|s| pages.get(s))
            .map(|secs| secs.iter().any(|s| matches!(s.kind, quicuts_manifest::SectionKind::Taskbar)))
            .unwrap_or(false);

        let pinned_app = pinned.filter(|p| pages.contains_key(*p)).map(str::to_string);
        OverlayStatePayload {
            platform: PLATFORM,
            apps,
            pages,
            selected,
            pinned_app,
            has_taskbar,
            combo_display_mode,
            hold_enabled: activation.hold_enabled,
            chord_enabled: activation.chord_enabled,
            chord: activation.chord,
            settings_chord_enabled: activation.settings_chord_enabled,
            settings_chord: activation.settings_chord,
        }
    }
}

/// Resolve a manifest's `Icon:` to a file next to the manifest. The value
/// must be a bare filename — anything path-like (absolute, separators,
/// parent components) is rejected so a manifest can never point outside
/// its own directory.
fn manifest_icon_path(lm: &quicuts_manifest::LoadedManifest) -> Option<PathBuf> {
    let name = lm.manifest.icon.as_deref()?;
    let ok = !name.is_empty()
        && !name.contains(['/', '\\'])
        && !name.contains("..")
        && !std::path::Path::new(name).is_absolute();
    if !ok {
        log::warn!("manifest {} has invalid Icon value {name:?}", lm.manifest.id);
        return None;
    }
    Some(lm.path.parent()?.join(name))
}

#[cfg(windows)]
fn ptsg_runtime_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|l| PathBuf::from(l).join("Microsoft/WinGet/KeyboardShortcuts"))
        .filter(|p| p.exists())
}

#[cfg(not(windows))]
fn ptsg_runtime_dir() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::foreground_entry;
    use quicuts_manifest::MatchKind::{Background, Exact, Hosted, Wildcard};

    /// A browser Quicuts has a manifest for: the host stays selected, and a
    /// hosted collection in the rail must not steal the default.
    #[test]
    fn host_with_a_manifest_keeps_the_default() {
        let e = [(Exact, "Google.Chrome"), (Hosted, "Google.Gmail"), (Wildcard, "Apple.System")];
        assert_eq!(foreground_entry(&e, None), Some(0));
    }

    /// Regression (reported on a real Mac): Firefox has no entry in
    /// `manifests-mac/`, so the first non-background match is Gmail — which
    /// presented Gmail's shortcuts on a new-tab page, at every URL. The
    /// default must fall through to the wildcard instead.
    #[test]
    fn host_without_a_manifest_does_not_default_to_a_hosted_collection() {
        let e = [(Hosted, "Google.Gmail"), (Hosted, "Yahoo.YahooMail"), (Wildcard, "Apple.System")];
        assert_eq!(foreground_entry(&e, None), Some(2));
    }

    /// ...but detection still wins when it has a match, manifest or not.
    #[test]
    fn title_match_selects_the_hosted_collection() {
        let e = [(Hosted, "Google.Gmail"), (Hosted, "Yahoo.YahooMail"), (Wildcard, "Apple.System")];
        assert_eq!(foreground_entry(&e, Some("Yahoo.YahooMail")), Some(1));
        let with_host = [(Exact, "Google.Chrome"), (Hosted, "Google.Gmail")];
        assert_eq!(foreground_entry(&with_host, Some("Google.Gmail")), Some(1));
    }

    /// A stale or uninstalled title match falls back to the same rule
    /// rather than selecting nothing.
    #[test]
    fn unknown_title_match_falls_back() {
        let e = [(Exact, "Google.Chrome"), (Hosted, "Google.Gmail")];
        assert_eq!(foreground_entry(&e, Some("Acme.Nope")), Some(0));
    }

    /// Nothing but hosted/background: no foreground entry, so the panel
    /// shows the unsupported-app placeholder instead of a web app.
    #[test]
    fn no_exact_or_wildcard_means_no_foreground_entry() {
        let e = [(Hosted, "Google.Gmail"), (Background, "+WindowsNT.Shell")];
        assert_eq!(foreground_entry(&e, None), None);
    }
}
