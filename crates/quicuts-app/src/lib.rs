//! Quicuts main application: tray-resident host that owns the overlay,
//! settings, and badge windows, the manifest engine, and the agent
//! supervisor. All system-level input access lives in the sidecar, never here.

mod agent;
mod app_state;
mod appname;
mod commands;
mod customizations;
mod engine;
mod icons;
mod overlay;
mod pinned;
mod procs;
mod settings;
mod tray;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use app_state::AppState;
use engine::Engine;
use pinned::PinStore;
use settings::Settings;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch just opens settings.
            tray::open_settings(app);
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        // Persist settings-window geometry across runs. Overlay/badges are
        // positioned programmatically every show, so they're excluded; the
        // VISIBLE flag is too, or the hidden pre-created settings window
        // would pop open at launch.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .with_denylist(&["overlay", "badges"])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::hide_overlay,
            commands::dismiss_overlay,
            commands::set_filter_active,
            commands::set_overlay_modal,
            commands::add_customization,
            commands::remove_customization,
            commands::set_default_redefined,
            commands::set_combo_display_mode,
            commands::open_settings,
            commands::select_app,
            commands::toggle_pin,
            commands::set_pinned_app,
            commands::get_settings,
            commands::set_settings,
            commands::adjust_font_scale,
            commands::get_last_browser_title,
            commands::list_hosted_collections,
        ])
        .on_window_event(|window, event| match window.label() {
            "overlay" => match event {
                WindowEvent::Focused(false) => {
                    let app = window.app_handle();
                    // Sticky mode: keep the panel up while the user returns to
                    // their app to try the shortcuts.
                    let auto_hide = app.state::<AppState>().settings.lock().unwrap().auto_hide;
                    if auto_hide {
                        overlay::hide(app);
                    }
                    // Either way, tell the agent the overlay is no longer
                    // active, so it stops swallowing Esc globally while the
                    // user types in their own app (sticky mode).
                    agent::send(
                        app,
                        &quicuts_proto::AgentCommand::SetOverlayVisible { visible: false },
                    );
                }
                // Dragging the panel edge persists the width (ADR 0005).
                WindowEvent::Resized(size) => {
                    overlay::on_resized(window.app_handle(), *size);
                }
                _ => {}
            },
            // Keep the settings/badges windows alive across closes so they can
            // be reopened; hide instead of destroy.
            "settings" | "badges" => {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if window.label() == "settings" {
                        // Hides and persists geometry (the window is never
                        // destroyed, so save now rather than only on app exit).
                        tray::close_settings(window.app_handle());
                    } else {
                        let _ = window.hide();
                    }
                }
            }
            _ => {}
        })
        .setup(|app| {
            // Tray-only on macOS: no Dock icon, no ⌘Tab entry.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle().clone();
            let config_dir = handle
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            std::fs::create_dir_all(&config_dir).ok();

            let settings = Settings::load(&config_dir);
            let user_manifests = config_dir.join("manifests");
            std::fs::create_dir_all(&user_manifests).ok();
            let bundled = bundled_manifests_dir(&handle);
            log::info!("bundled manifests: {}", bundled.display());

            let engine = Engine::new(bundled, user_manifests);
            let pins = PinStore::load(config_dir.clone());
            let customs = customizations::CustomStore::new(&config_dir);

            app.manage(AppState {
                config_dir,
                settings: Mutex::new(settings),
                engine: Mutex::new(engine),
                pins: Mutex::new(pins),
                customs: Mutex::new(customs),
                foreground: Mutex::new(None),
                selected: Mutex::new(None),
                pinned_app: Mutex::new(None),
                title_matched: Mutex::new(None),
                last_browser_title: Mutex::new(None),
                agent_child: Mutex::new(None),
                last_taskbar: Mutex::new(None),
                overlay_visible: Mutex::new(false),
                filter_active: Mutex::new(false),
                overlay_modal: Mutex::new("none".into()),
                panel_expected_w: Mutex::new(None),
                panel_resize_gen: Mutex::new(0),
            });

            tray::build(&handle)?;
            apply_autostart(&handle);
            agent::start(handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building Quicuts")
        .run(|_app, event| {
            // Keep running with no visible window (tray-resident).
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // allow explicit exit only via tray Quit (app.exit)
            }
        });
}

/// Enable/disable OS autostart to match settings.
pub fn apply_autostart(app: &tauri::AppHandle) {
    let want = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .launch_at_login;
    let mgr = app.autolaunch();
    let is_on = mgr.is_enabled().unwrap_or(false);
    if want && !is_on {
        let _ = mgr.enable();
    } else if !want && is_on {
        let _ = mgr.disable();
    }
}

/// Resolve the bundled-manifests directory across dev and installed layouts.
fn bundled_manifests_dir(app: &tauri::AppHandle) -> PathBuf {
    // macOS ships its own manifest set and must never fall back to the
    // Windows one: that would put Win+E / File Explorer entries on every mac
    // panel, which is exactly what the sibling directory exists to prevent.
    // A missing `manifests-mac` is an empty store, not the wrong store.
    #[cfg(target_os = "macos")]
    {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(res) = app.path().resource_dir() {
            candidates.push(res.join("manifests-mac"));
            candidates.push(res.join("_up_/_up_/manifests-mac"));
        }
        // Dev workspace fallback.
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests-mac"));
        first_existing(candidates)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(res) = app.path().resource_dir() {
            candidates.push(res.join("manifests"));
            candidates.push(res.join("_up_/_up_/manifests")); // tauri "../../" resource mapping
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("manifests"));
                candidates.push(dir.join("../manifests"));
            }
        }
        // Dev workspace fallback.
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../manifests"));
        first_existing(candidates)
    }
}

/// First candidate that exists, else the last one (the dev fallback) so the
/// caller has a path to log.
fn first_existing(mut candidates: Vec<PathBuf>) -> PathBuf {
    candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| candidates.pop().expect("at least one candidate"))
}
