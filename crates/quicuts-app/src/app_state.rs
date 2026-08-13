//! Shared, Tauri-managed application state.

use std::path::PathBuf;
use std::sync::Mutex;

use quicuts_proto::{ForegroundInfo, TaskbarSnapshot};
use tauri_plugin_shell::process::CommandChild;

use crate::customizations::CustomStore;
use crate::engine::Engine;
use crate::pinned::PinStore;
use crate::settings::Settings;

pub struct AppState {
    pub config_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub engine: Mutex<Engine>,
    pub pins: Mutex<PinStore>,
    /// Per-app user shortcut customizations (files under customizations/).
    pub customs: Mutex<CustomStore>,
    /// Foreground app captured at the last activation (keys the page).
    pub foreground: Mutex<Option<ForegroundInfo>>,
    /// Currently selected rail app.
    pub selected: Mutex<Option<String>>,
    /// Rail app pinned by the user: stays in the rail and re-selects on
    /// every foreground change until unpinned. At most one; not persisted.
    pub pinned_app: Mutex<Option<String>>,
    /// Hosted collection currently auto-detected via TitleMatch
    /// (experimental title detection); None when off or no match.
    pub title_matched: Mutex<Option<String>>,
    /// Title of the most recent browser-class foreground window, kept for
    /// the settings capture flow (the browser loses foreground when the
    /// settings window opens). In-memory only; never persisted.
    pub last_browser_title: Mutex<Option<String>>,
    /// Handle to the running sidecar, for writing commands.
    pub agent_child: Mutex<Option<CommandChild>>,
    pub last_taskbar: Mutex<Option<TaskbarSnapshot>>,
    pub overlay_visible: Mutex<bool>,
    /// Whether the overlay's filter box currently has text (reported by the
    /// webview), so Esc can clear it before closing the panel.
    pub filter_active: Mutex<bool>,
    /// Which modal layer the overlay has open ("none" | "dialog" |
    /// "capture"), reported by the webview; Esc peels layers before the
    /// filter/close logic runs.
    pub overlay_modal: Mutex<String>,
    /// Logical overlay width we last set ourselves; a Resized event with a
    /// different width is the user dragging the panel edge (ADR 0005).
    pub panel_expected_w: Mutex<Option<u32>>,
    /// Bumped per user-resize event; the debounced save only fires when it
    /// is still current after the delay.
    pub panel_resize_gen: Mutex<u64>,
}
