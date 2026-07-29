//! IPC contract between the Quicuts app and its system-agent sidecars.
//!
//! Transport: NDJSON — one JSON object per line over the sidecar's
//! stdin (commands) / stdout (events). The protocol version is negotiated
//! via [`AgentEvent::Ready`].
//!
//! Privacy invariant: no message may ever carry the identity of a key the
//! user pressed. The only key data on the wire is the activation-chord
//! *configuration* flowing app -> agent. Agents report semantic transitions
//! only (activated / released / dismissed), never keystrokes.

use serde::{Deserialize, Serialize};

pub const PROTO_VERSION: u32 = 1;

/// Capability strings advertised in [`AgentEvent::Ready`].
pub mod caps {
    pub const HOLD: &str = "hold";
    pub const CHORD: &str = "chord";
    pub const FOREGROUND: &str = "foreground";
    pub const TASKBAR: &str = "taskbar";
    /// Can watch foreground-window title changes (experimental title
    /// detection for hosted collections).
    pub const TITLE: &str = "title";
}

/// An activation chord: modifier flags plus one non-modifier key
/// (a platform virtual-key code; VK_* on Windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChordSpec {
    #[serde(default)]
    pub win: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    pub vk: u32,
}

impl Default for ChordSpec {
    /// Overlay-toggle default. Windows keeps the PTSG default Win+Shift+/
    /// (VK_OEM_2). macOS uses ⌃⌘/ — ⌘⇧/, the literal equivalent, is the
    /// system Help-menu shortcut and must not be taken. (`win` means ⌘ on
    /// macOS; the VK stays 0xBF because the settings UI captures Windows-
    /// style keycodes on both platforms — agents translate internally.)
    fn default() -> Self {
        if cfg!(target_os = "macos") {
            Self { win: true, ctrl: true, shift: false, alt: false, vk: 0xBF }
        } else {
            Self { win: true, ctrl: false, shift: true, alt: false, vk: 0xBF }
        }
    }
}

impl ChordSpec {
    /// Default settings-window toggle: Ctrl+, on Windows, ⌘, on macOS (the
    /// universal prefs shortcut; VK_OEM_COMMA either way). Handled inside
    /// the app's own focused windows, never by the agent hook.
    pub fn settings_default() -> Self {
        if cfg!(target_os = "macos") {
            Self { win: true, ctrl: false, shift: false, alt: false, vk: 0xBC }
        } else {
            Self { win: false, ctrl: true, shift: false, alt: false, vk: 0xBC }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ForegroundInfo {
    pub pid: u32,
    /// Full image path, when the process could be opened.
    pub exe_path: Option<String>,
    /// Basename of `exe_path`, e.g. `chrome.exe`.
    pub exe_name: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskbarButton {
    /// 1-based position; badge label ("0" is rendered for >= 10 by the UI).
    pub index: u32,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskbarSnapshot {
    pub edge: Edge,
    pub taskbar_rect: Rect,
    pub buttons: Vec<TaskbarButton>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DismissReason {
    Esc,
    /// A shortcut chord (e.g. Win+E) was passed through to the OS while the
    /// overlay was up; the overlay should hide.
    Passthrough,
    /// The activation chord was pressed again while visible (toggle off).
    ChordAgain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FatalKind {
    HookInstallFailed,
    /// macOS: Accessibility / Input Monitoring permission missing.
    PermissionRequired,
    AlreadyRunning,
    Other,
}

/// App -> agent, one per line on the sidecar's stdin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentCommand {
    /// Full-replace configuration; sent at startup and on every settings save.
    Configure {
        hold_enabled: bool,
        hold_ms: u32,
        chord_enabled: bool,
        chord: ChordSpec,
        /// Lowercase exe names (".exe" optional) for which hold-activation
        /// must not arm.
        excluded_exes: Vec<String>,
        /// Watch foreground-window title changes and re-emit
        /// `ForegroundChanged` (debounced) when they settle. Experimental;
        /// off by default. Note: still only window metadata on the wire —
        /// never key identities.
        #[serde(default)]
        title_events_enabled: bool,
    },
    /// The app's acknowledgement that the overlay is actually shown/hidden;
    /// gates the agent's suppression rules (Esc swallowing etc.).
    SetOverlayVisible { visible: bool },
    /// Request one taskbar-button geometry snapshot.
    QueryTaskbar,
    SubscribeForeground { enabled: bool },
    Ping,
    Shutdown,
}

/// Agent -> app, one per line on the sidecar's stdout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentEvent {
    Ready {
        proto_version: u32,
        caps: Vec<String>,
    },
    /// The hold threshold elapsed with the modifier still down.
    /// `foreground` is the window that was foreground *before* activation.
    HoldActivated { foreground: Option<ForegroundInfo> },
    /// The held modifier was released.
    HoldReleased,
    ChordActivated { foreground: Option<ForegroundInfo> },
    Dismissed { reason: DismissReason },
    ForegroundChanged { foreground: ForegroundInfo },
    /// A shell tray flyout (the "show hidden icons" overflow) opened or
    /// closed. It is not even topmost, so an always-on-top overlay covers
    /// it; the app slots the overlay directly beneath it while it is open.
    /// Not reported as a `ForegroundChanged`.
    ShellFlyoutChanged {
        open: bool,
        /// Native handle of the flyout window while open (Windows: HWND),
        /// so the app can position the overlay relative to it.
        #[serde(default)]
        window: Option<u64>,
    },
    Taskbar(TaskbarSnapshot),
    Pong,
    Fatal { kind: FatalKind, message: String },
}

/// Serialize a message as one NDJSON line (no trailing newline).
pub fn to_line<T: Serialize>(msg: &T) -> serde_json::Result<String> {
    serde_json::to_string(msg)
}

/// Parse one NDJSON line.
pub fn from_line<'a, T: Deserialize<'a>>(line: &'a str) -> serde_json::Result<T> {
    serde_json::from_str(line.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_command() {
        let cmd = AgentCommand::Configure {
            hold_enabled: true,
            hold_ms: 900,
            chord_enabled: true,
            chord: ChordSpec::default(),
            excluded_exes: vec!["outlook".into()],
            title_events_enabled: true,
        };
        let line = to_line(&cmd).unwrap();
        assert!(!line.contains('\n'));
        assert_eq!(from_line::<AgentCommand>(&line).unwrap(), cmd);
    }

    #[test]
    fn configure_line_without_title_flag_still_parses() {
        // A pre-ADR-0003 app must keep configuring a newer agent.
        let old = r#"{"type":"configure","hold_enabled":true,"hold_ms":900,"chord_enabled":true,"chord":{"win":true,"shift":true,"vk":191},"excluded_exes":[]}"#;
        let cmd: AgentCommand = from_line(old).unwrap();
        match cmd {
            AgentCommand::Configure { title_events_enabled, .. } => {
                assert!(!title_events_enabled)
            }
            _ => panic!("expected Configure"),
        }
    }

    #[test]
    fn roundtrip_event() {
        let ev = AgentEvent::HoldActivated {
            foreground: Some(ForegroundInfo {
                pid: 42,
                exe_path: Some(r"C:\Windows\notepad.exe".into()),
                exe_name: Some("notepad.exe".into()),
                title: Some("Untitled".into()),
            }),
        };
        let line = to_line(&ev).unwrap();
        assert_eq!(from_line::<AgentEvent>(&line).unwrap(), ev);
    }

    #[test]
    fn tag_shape_is_stable() {
        let line = to_line(&AgentEvent::Pong).unwrap();
        assert_eq!(line, r#"{"type":"pong"}"#);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_defaults_are_ctrl_cmd_slash_and_cmd_comma() {
        // ⌃⌘/ (win == ⌘): ⌘⇧/ is the system Help-menu shortcut, keep off it.
        let chord = ChordSpec::default();
        assert!(chord.win && chord.ctrl && !chord.shift && !chord.alt);
        assert_eq!(chord.vk, 0xBF);
        // ⌘,
        let settings = ChordSpec::settings_default();
        assert!(settings.win && !settings.ctrl && !settings.shift && !settings.alt);
        assert_eq!(settings.vk, 0xBC);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_mac_defaults_keep_ptsg_values() {
        let chord = ChordSpec::default();
        assert!(chord.win && !chord.ctrl && chord.shift && !chord.alt);
        assert_eq!(chord.vk, 0xBF);
        let settings = ChordSpec::settings_default();
        assert!(!settings.win && settings.ctrl && !settings.shift && !settings.alt);
        assert_eq!(settings.vk, 0xBC);
    }
}
