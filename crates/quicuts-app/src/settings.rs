//! Typed settings with atomic persistence. Field names are camelCase to match
//! the Svelte settings UI directly.

use std::path::PathBuf;

use quicuts_proto::ChordSpec;
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activation {
    pub hold_enabled: bool,
    pub hold_ms: u32,
    pub chord_enabled: bool,
    pub chord: ChordSpec,
    /// Global chord that opens/closes the settings window.
    #[serde(default = "default_true")]
    pub settings_chord_enabled: bool,
    #[serde(default = "ChordSpec::settings_default")]
    pub settings_chord: ChordSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    pub theme: String,
    pub overlay_style: String,
    pub panel_edge: String,
    /// Panel background opacity, 0.0 (fully transparent) – 1.0 (opaque).
    #[serde(default = "default_panel_opacity")]
    pub panel_opacity: f64,
    /// Accessibility zoom for the overlay window's text and keycaps,
    /// 0.8–2.0 on a 5% grid (ADR 0005).
    #[serde(default = "default_font_scale")]
    pub font_scale: f64,
    /// User-dragged overlay width in logical px; never below the classic
    /// 586. Only changed by dragging the panel edge, so `set_settings`
    /// keeps the live value rather than the UI's round-tripped copy.
    #[serde(default = "default_panel_width")]
    pub panel_width: f64,
    /// When true, the effective panel width is panel_width × font_scale.
    #[serde(default)]
    pub auto_width_resize: bool,
}

fn default_panel_opacity() -> f64 {
    0.82
}

fn default_font_scale() -> f64 {
    1.0
}

fn default_panel_width() -> f64 {
    586.0
}

pub const FONT_SCALE_MIN: f64 = 0.8;
pub const FONT_SCALE_MAX: f64 = 2.0;

/// Clamp to the supported range and snap to the 5% grid so persisted
/// values stay clean (e.g. 1.15, never 1.1500000000000001).
pub fn clamp_font_scale(scale: f64) -> f64 {
    (scale.clamp(FONT_SCALE_MIN, FONT_SCALE_MAX) * 20.0).round() / 20.0
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub schema_version: u32,
    pub activation: Activation,
    pub appearance: Appearance,
    pub excluded_exes: Vec<String>,
    pub launch_at_login: bool,
    pub taskbar_badges: bool,
    /// Hide the panel when it loses focus. Off = sticky: the panel stays up
    /// while the user returns to their app to try the shortcuts.
    #[serde(default = "default_true")]
    pub auto_hide: bool,
    /// When the filter box has text, the first Esc clears it and only the
    /// next Esc closes the panel.
    #[serde(default = "default_true")]
    pub esc_clears_filter: bool,
    /// Which combos the panel shows: "default" | "custom" | "all" |
    /// "customElseDefault". Cycled by the panel's footer switch.
    #[serde(default = "default_combo_display_mode")]
    pub combo_display_mode: String,
    /// Experimental: the agent watches foreground title changes and hosted
    /// collections auto-select on TitleMatch (ADR 0003).
    #[serde(default)]
    pub title_detection: bool,
    /// Extra browser exe names extending the built-in "browser" host class.
    #[serde(default)]
    pub extra_browser_exes: Vec<String>,
    /// User-captured title signatures bound to hosted collections (part of
    /// experimental title detection). A binding's pattern beats any manifest
    /// `TitleMatch` on the same title; bindings referencing an uninstalled
    /// manifest are kept (shown as dangling in the UI), never auto-deleted.
    #[serde(default)]
    pub title_bindings: Vec<quicuts_manifest::TitleBinding>,
}

fn default_combo_display_mode() -> String {
    "all".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            activation: Activation {
                hold_enabled: true,
                hold_ms: 900,
                chord_enabled: true,
                chord: ChordSpec::default(),
                settings_chord_enabled: true,
                settings_chord: ChordSpec::settings_default(),
            },
            appearance: Appearance {
                theme: "system".into(),
                overlay_style: "panel".into(),
                panel_edge: "right".into(),
                panel_opacity: default_panel_opacity(),
                font_scale: default_font_scale(),
                panel_width: default_panel_width(),
                auto_width_resize: false,
            },
            excluded_exes: Vec::new(),
            launch_at_login: false,
            taskbar_badges: true,
            auto_hide: true,
            esc_clears_filter: true,
            combo_display_mode: default_combo_display_mode(),
            title_detection: false,
            extra_browser_exes: Vec::new(),
            title_bindings: Vec::new(),
        }
    }
}

impl Settings {
    pub fn load(dir: &PathBuf) -> Self {
        let path = dir.join("settings.json");
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                log::warn!("settings parse failed ({e}); using defaults");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    /// Atomic write (temp + rename) so a crash can't leave a half-written file.
    pub fn save(&self, dir: &PathBuf) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("settings.json");
        let tmp = dir.join("settings.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Build the agent Configure command from current settings.
    pub fn to_configure(&self) -> quicuts_proto::AgentCommand {
        quicuts_proto::AgentCommand::Configure {
            hold_enabled: self.activation.hold_enabled,
            hold_ms: self.activation.hold_ms,
            chord_enabled: self.activation.chord_enabled,
            chord: self.activation.chord,
            excluded_exes: self.excluded_exes.clone(),
            title_events_enabled: self.title_detection,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_conservative() {
        let s = Settings::default();
        assert!(!s.title_detection, "experimental features must default off");
        assert!(s.extra_browser_exes.is_empty());
    }

    #[test]
    fn old_settings_json_still_parses() {
        // A settings.json written before ADR 0003 has none of the new fields.
        let old = r#"{
            "schemaVersion": 1,
            "activation": {"holdEnabled": true, "holdMs": 900, "chordEnabled": true,
                           "chord": {"win": true, "shift": true, "vk": 191}},
            "appearance": {"theme": "system", "overlayStyle": "panel", "panelEdge": "right"},
            "excludedExes": [], "launchAtLogin": false, "taskbarBadges": true
        }"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert!(!s.title_detection);
        assert!(s.extra_browser_exes.is_empty());
        // ADR 0005 fields default when absent.
        assert_eq!(s.appearance.font_scale, 1.0);
        assert_eq!(s.appearance.panel_width, 586.0);
        assert!(!s.appearance.auto_width_resize);
    }

    #[test]
    fn font_scale_clamps_and_snaps() {
        assert_eq!(clamp_font_scale(0.5), 0.8);
        assert_eq!(clamp_font_scale(3.0), 2.0);
        assert_eq!(clamp_font_scale(1.0 + 0.1 + 0.1 + 0.1), 1.3); // no float drift
        assert_eq!(clamp_font_scale(1.13), 1.15); // snaps to the 5% grid
    }

    #[test]
    fn round_trip_and_configure() {
        let mut s = Settings::default();
        s.title_detection = true;
        s.extra_browser_exes = vec!["thorium.exe".into()];
        s.title_bindings = vec![quicuts_manifest::TitleBinding {
            pattern: "Carbon Register Mail".into(),
            manifest_id: "Google.Gmail".into(),
        }];
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"titleDetection\":true"));
        // camelCase on the wire, like every settings field the UI touches.
        assert!(json.contains("\"manifestId\":\"Google.Gmail\""));
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert!(back.title_detection);
        assert_eq!(back.extra_browser_exes, vec!["thorium.exe"]);
        assert_eq!(back.title_bindings, s.title_bindings);
        match back.to_configure() {
            quicuts_proto::AgentCommand::Configure { title_events_enabled, .. } => {
                assert!(title_events_enabled)
            }
            _ => panic!("expected Configure"),
        }
    }
}
