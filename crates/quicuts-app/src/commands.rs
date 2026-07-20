//! Tauri commands invoked from the frontend.

use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::settings::Settings;
use crate::{agent, overlay};

#[tauri::command]
pub fn hide_overlay(app: AppHandle) {
    overlay::hide(&app);
    agent::send(&app, &quicuts_proto::AgentCommand::SetOverlayVisible { visible: false });
}

/// Esc handler for when the webview itself receives the key (e.g. browser
/// dev); routes through the same two-step logic as the agent's `Dismissed`.
#[tauri::command]
pub fn dismiss_overlay(app: AppHandle) {
    overlay::dismiss(&app);
}

/// The overlay reports filter box empty/non-empty transitions so Esc can
/// decide between clearing the filter and closing the panel.
#[tauri::command]
pub fn set_filter_active(app: AppHandle, active: bool) {
    *app.state::<AppState>().filter_active.lock().unwrap() = active;
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    // Toggle: clicking the Settings button while the dialog is already open
    // closes it. A minimized window doesn't count as open — restore it instead.
    if let Some(w) = app.get_webview_window("settings") {
        if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) {
            crate::tray::close_settings(&app);
            return;
        }
    }
    // With auto-hide on, drop the overlay so settings isn't behind it; in
    // sticky mode keep the panel up (settings takes focus regardless).
    let auto_hide = app.state::<AppState>().settings.lock().unwrap().auto_hide;
    if auto_hide {
        overlay::hide(&app);
    }
    agent::send(&app, &quicuts_proto::AgentCommand::SetOverlayVisible { visible: false });
    crate::tray::open_settings(&app);
}

#[tauri::command]
pub fn select_app(app: AppHandle, manifest_id: String) {
    *app.state::<AppState>().selected.lock().unwrap() = Some(manifest_id);
    overlay::push_state(&app);
}

/// Pin (or unpin, with None) an app in the rail: it stays listed and
/// re-selects on every foreground change until unpinned.
#[tauri::command]
pub fn set_pinned_app(app: AppHandle, manifest_id: Option<String>) {
    // The unsupported-app placeholder is not pinnable (ADR 0004): its
    // synthetic id names no manifest, so a persisted pin would resurrect
    // an empty page forever. The UI hides the affordance; this is the
    // backstop.
    if manifest_id.as_deref().is_some_and(|id| id.starts_with("unsupported:")) {
        return;
    }
    let state = app.state::<AppState>();
    *state.pinned_app.lock().unwrap() = manifest_id.clone();
    if let Some(id) = manifest_id {
        *state.selected.lock().unwrap() = Some(id);
    }
    overlay::push_state(&app);
}

#[tauri::command]
pub fn toggle_pin(app: AppHandle, manifest_id: String, entry_key: String) {
    {
        let state = app.state::<AppState>();
        state.pins.lock().unwrap().toggle(&manifest_id, &entry_key);
    }
    overlay::push_state(&app);
}

/// The overlay reports which modal layer is open ("none" | "dialog" |
/// "capture") so Esc peels layers before the filter/close logic runs.
#[tauri::command]
pub fn set_overlay_modal(app: AppHandle, modal: String) {
    *app.state::<AppState>().overlay_modal.lock().unwrap() = modal;
}

/// Record a captured customization. The webview sends the chords it captured
/// as KeyCombo JSON; they're stored in the human-editable string form.
#[tauri::command]
pub fn add_customization(
    app: AppHandle,
    manifest_id: String,
    entry_name: String,
    chords: Vec<quicuts_manifest::KeyCombo>,
) -> Result<(), String> {
    if chords.is_empty() {
        return Err("empty key combination".into());
    }
    let combo = quicuts_manifest::custom::format_combo_seq(&chords);
    let state = app.state::<AppState>();
    state
        .customs
        .lock()
        .unwrap()
        .update(&manifest_id, |c| c.add_custom(&entry_name, &combo));
    overlay::push_state(&app);
    Ok(())
}

/// `combo` is the stored string (the `raw` field of a custom binding).
#[tauri::command]
pub fn remove_customization(app: AppHandle, manifest_id: String, entry_name: String, combo: String) {
    let state = app.state::<AppState>();
    state
        .customs
        .lock()
        .unwrap()
        .update(&manifest_id, |c| c.remove_custom(&entry_name, &combo));
    overlay::push_state(&app);
}

/// Mark (or unmark) a default binding as reassigned in the app itself.
#[tauri::command]
pub fn set_default_redefined(
    app: AppHandle,
    manifest_id: String,
    entry_name: String,
    chords: Vec<quicuts_manifest::KeyCombo>,
    redefined: bool,
) {
    let combo = quicuts_manifest::custom::format_combo_seq(&chords);
    let state = app.state::<AppState>();
    state
        .customs
        .lock()
        .unwrap()
        .update(&manifest_id, |c| c.set_redefined(&entry_name, &combo, redefined));
    overlay::push_state(&app);
}

/// Persist the footer switch position and re-push the overlay state.
#[tauri::command]
pub fn set_combo_display_mode(app: AppHandle, mode: String) {
    let state = app.state::<AppState>();
    {
        let mut s = state.settings.lock().unwrap();
        s.combo_display_mode = mode;
        if let Err(e) = s.save(&state.config_dir) {
            log::error!("failed to save settings: {e}");
        }
    }
    overlay::push_state(&app);
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let state = app.state::<AppState>();
    settings.save(&state.config_dir).map_err(|e| e.to_string())?;
    let cmd = settings.to_configure();
    if !settings.title_detection {
        // Detection switched off: drop any live title match so push_state
        // restores host-based selection.
        *state.title_matched.lock().unwrap() = None;
    }
    *state.settings.lock().unwrap() = settings;
    // Live-reload: push config to the agent, refresh appearance, and
    // re-sync the overlay (arms/disarms taskbar badges).
    agent::send(&app, &cmd);
    agent::emit_appearance(&app);
    overlay::push_state(&app);
    crate::apply_autostart(&app);
    Ok(())
}
