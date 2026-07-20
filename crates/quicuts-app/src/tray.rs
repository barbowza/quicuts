//! System-tray icon + menu.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::{agent, overlay};

pub fn build(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let show = MenuItem::with_id(app, "show", "Show shortcuts", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let restart = MenuItem::with_id(app, "restart", "Restart agent", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Quicuts", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &settings, &restart, &quit])?;

    TrayIconBuilder::with_id("quicuts-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Quicuts")
        .menu(&menu)
        // The overlay is always-on-top, and the native tray menu is owned by
        // a hidden non-topmost window, so the menu would pop *under* the
        // panel. There is no way to raise an HMENU above another topmost
        // window, so duck the panel on click-down, before the menu opens.
        // Click only — Move/Enter/Leave hover events must not hide it.
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button_state: MouseButtonState::Down, .. } = event {
                let app = tray.app_handle();
                if *app.state::<AppState>().overlay_visible.lock().unwrap() {
                    overlay::hide(app);
                    agent::send(
                        app,
                        &quicuts_proto::AgentCommand::SetOverlayVisible { visible: false },
                    );
                }
            }
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => overlay::show(app),
            "settings" => open_settings(app),
            "restart" => {
                if let Some(child) = app.state::<AppState>().agent_child.lock().unwrap().take() {
                    let _ = child.kill();
                }
                // The supervisor loop respawns on termination.
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)
}

pub fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    agent::emit_appearance(app);
}

/// Hide the settings window (it is never destroyed) and persist its geometry,
/// same as the titlebar-close path in `lib.rs`.
pub fn close_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.hide();
        use tauri_plugin_window_state::{AppHandleExt, StateFlags};
        let _ = app
            .save_window_state(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED);
    }
}

/// Reflect an agent-failure state in the tray tooltip.
pub fn set_error(app: &AppHandle, errored: bool) {
    if let Some(tray) = app.tray_by_id("quicuts-tray") {
        let tip = if errored { "Quicuts — agent stopped" } else { "Quicuts" };
        let _ = tray.set_tooltip(Some(tip));
    }
}
