//! Sidecar supervisor: spawn the agent, stream its NDJSON events, translate
//! them into overlay show/hide + view-model updates, and restart on crash.

use std::time::Duration;

use quicuts_proto::{AgentCommand, AgentEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::app_state::AppState;
use crate::{overlay, tray};

const MAX_RESTARTS: u32 = 5;

/// Spawn (or respawn) the agent sidecar and pump its events.
pub fn start(app: AppHandle) {
    spawn(app, 0);
}

fn spawn(app: AppHandle, attempt: u32) {
    tauri::async_runtime::spawn(async move {
        let sidecar = match app.shell().sidecar("quicuts-agent") {
            Ok(cmd) => cmd,
            Err(e) => {
                log::error!("cannot resolve sidecar: {e}");
                return;
            }
        };
        let (mut rx, child) = match sidecar.spawn() {
            Ok(pair) => pair,
            Err(e) => {
                log::error!("cannot spawn agent: {e}");
                schedule_restart(app, attempt);
                return;
            }
        };
        {
            let state = app.state::<AppState>();
            *state.agent_child.lock().unwrap() = Some(child);
        }
        log::info!("agent spawned (attempt {attempt})");

        let mut buf = String::new();
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    buf.push_str(&String::from_utf8_lossy(&bytes));
                    drain_lines(&app, &mut buf);
                }
                CommandEvent::Stderr(bytes) => {
                    // The agent only writes stderr for notable events; keep
                    // them visible at the default Info filter.
                    log::info!("agent: {}", String::from_utf8_lossy(&bytes).trim());
                }
                CommandEvent::Error(e) => log::error!("agent error: {e}"),
                CommandEvent::Terminated(payload) => {
                    log::warn!("agent terminated: {:?}", payload.code);
                    break;
                }
                _ => {}
            }
        }

        {
            let state = app.state::<AppState>();
            *state.agent_child.lock().unwrap() = None;
        }
        schedule_restart(app, attempt);
    });
}

fn schedule_restart(app: AppHandle, attempt: u32) {
    if attempt >= MAX_RESTARTS {
        log::error!("agent gave up after {MAX_RESTARTS} restarts");
        tray::set_error(&app, true);
        return;
    }
    let delay = Duration::from_millis(250u64 * (1 << attempt).min(20));
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(delay).await;
        spawn(app, attempt + 1);
    });
}

fn drain_lines(app: &AppHandle, buf: &mut String) {
    while let Some(nl) = buf.find('\n') {
        let line: String = buf.drain(..=nl).collect();
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match quicuts_proto::from_line::<AgentEvent>(line) {
            Ok(ev) => handle_event(app, ev),
            Err(e) => log::warn!("bad agent event: {e}: {line}"),
        }
    }
}

/// Decide the rail selection after a foreground report. Returns
/// `Some(next)` to replace the selection, `None` to keep the current one.
///
/// - Window switch (or activation reset): pin wins, else the title match,
///   else none — the host page falls out of build_state's fallback.
/// - Same window, the title match transitioned (tab switch under
///   experimental detection), no pin: follow the match in both directions.
/// - Same window, no transition: keep — a manual rail click survives
///   title churn (e.g. Gmail's unread count) that re-matches the same id.
fn next_selection(
    window_changed: bool,
    old_match: Option<&str>,
    new_match: Option<&str>,
    pinned: Option<&str>,
) -> Option<Option<String>> {
    if window_changed {
        Some(pinned.or(new_match).map(str::to_string))
    } else if new_match != old_match && pinned.is_none() {
        Some(new_match.map(str::to_string))
    } else {
        None
    }
}

/// Store a foreground report and derive title match + rail selection.
/// `reset` forces the window-switch selection rule (used at activation,
/// preserving the pre-ADR-0003 reset-to-pin behavior).
fn apply_foreground(app: &AppHandle, fg: quicuts_proto::ForegroundInfo, reset: bool) {
    let state = app.state::<AppState>();
    let (detection, extra) = {
        let s = state.settings.lock().unwrap();
        (s.title_detection, s.extra_browser_exes.clone())
    };
    let new_match = if detection {
        let hc = quicuts_manifest::HostClasses::with_extensions(&extra);
        state.engine.lock().unwrap().title_match(&fg, &hc)
    } else {
        None
    };
    let window_changed = reset
        || state
            .foreground
            .lock()
            .unwrap()
            .as_ref()
            .map(|p| p.pid != fg.pid || p.exe_name != fg.exe_name)
            .unwrap_or(true);
    let old_match =
        std::mem::replace(&mut *state.title_matched.lock().unwrap(), new_match.clone());
    let pinned = state.pinned_app.lock().unwrap().clone();
    if let Some(next) = next_selection(
        window_changed,
        old_match.as_deref(),
        new_match.as_deref(),
        pinned.as_deref(),
    ) {
        *state.selected.lock().unwrap() = next;
    }
    *state.foreground.lock().unwrap() = Some(fg);
}

fn handle_event(app: &AppHandle, ev: AgentEvent) {
    let state = app.state::<AppState>();
    match ev {
        AgentEvent::Ready { proto_version, .. } => {
            log::info!("agent ready (proto {proto_version})");
            let cmd = state.settings.lock().unwrap().to_configure();
            send(app, &cmd);
        }
        AgentEvent::ForegroundChanged { foreground } => {
            apply_foreground(app, foreground, false);
            // Pre-assemble so activation is instant, but do not show.
            overlay::push_state(app);
        }
        AgentEvent::HoldActivated { foreground } => {
            if let Some(fg) = foreground {
                apply_foreground(app, fg, true);
            } else {
                *state.selected.lock().unwrap() = state.pinned_app.lock().unwrap().clone();
            }
            overlay::show(app);
            send(app, &AgentCommand::SetOverlayVisible { visible: true });
        }
        // The chord cycles: hidden -> show+focus; visible but unfocused
        // (sticky panel, user went back to their app) -> refocus the panel;
        // visible and focused -> hide, returning focus to the previous app.
        AgentEvent::ChordActivated { foreground } => {
            let window = app.get_webview_window("overlay");
            // Branch on our own flag, not `is_visible()`: hide() dispatches
            // fire-and-forget to the main thread, so the OS getter can still
            // report the pre-hide state when a chord lands right after a
            // hold-release. `overlay_visible` is updated synchronously by
            // overlay::show/hide, and events are drained serially (#9).
            let visible = *state.overlay_visible.lock().unwrap();
            let focused = window
                .as_ref()
                .map(|w| w.is_focused().unwrap_or(false))
                .unwrap_or(false);
            if visible && focused {
                overlay::hide(app);
                send(app, &AgentCommand::SetOverlayVisible { visible: false });
            } else if visible {
                if let Some(w) = window {
                    let _ = w.set_focus();
                }
                send(app, &AgentCommand::SetOverlayVisible { visible: true });
            } else {
                if let Some(fg) = foreground {
                    apply_foreground(app, fg, true);
                } else {
                    *state.selected.lock().unwrap() =
                        state.pinned_app.lock().unwrap().clone();
                }
                overlay::show(app);
                send(app, &AgentCommand::SetOverlayVisible { visible: true });
            }
        }
        AgentEvent::HoldReleased => {
            overlay::hide(app);
            send(app, &AgentCommand::SetOverlayVisible { visible: false });
        }
        // Esc: may just clear the filter box instead of closing (two-step).
        AgentEvent::Dismissed { .. } => overlay::dismiss(app),
        AgentEvent::ShellFlyoutChanged { open, window } => {
            overlay::duck_under_flyout(app, open, window);
        }
        AgentEvent::Taskbar(snap) => {
            log::info!("taskbar snapshot: {} buttons", snap.buttons.len());
            *state.last_taskbar.lock().unwrap() = Some(snap.clone());
            overlay::update_badges(app, &snap);
        }
        AgentEvent::Fatal { kind, message } => {
            log::error!("agent fatal {kind:?}: {message}");
            tray::set_error(app, true);
        }
        AgentEvent::Pong => {}
        _ => {}
    }
}

/// Serialize and write one command to the agent's stdin.
pub fn send(app: &AppHandle, cmd: &AgentCommand) {
    let state = app.state::<AppState>();
    let mut guard = state.agent_child.lock().unwrap();
    if let Some(child) = guard.as_mut() {
        if let Ok(mut line) = quicuts_proto::to_line(cmd) {
            line.push('\n');
            if let Err(e) = child.write(line.as_bytes()) {
                log::warn!("failed writing to agent: {e}");
            }
        }
    }
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppearancePayload {
    theme: String,
    panel_opacity: f64,
    font_scale: f64,
}

#[cfg(test)]
mod tests {
    use super::next_selection;

    #[test]
    fn window_switch_resets_pin_first_then_match() {
        assert_eq!(next_selection(true, None, None, None), Some(None));
        assert_eq!(
            next_selection(true, None, Some("Gmail"), None),
            Some(Some("Gmail".into()))
        );
        assert_eq!(
            next_selection(true, None, Some("Gmail"), Some("Pin")),
            Some(Some("Pin".into()))
        );
    }

    #[test]
    fn same_window_transitions_follow_match() {
        // Gained -> auto-select; lost -> revert (None falls back to host).
        assert_eq!(
            next_selection(false, None, Some("Gmail"), None),
            Some(Some("Gmail".into()))
        );
        assert_eq!(next_selection(false, Some("Gmail"), None, None), Some(None));
        // Changed site-to-site.
        assert_eq!(
            next_selection(false, Some("Gmail"), Some("Docs"), None),
            Some(Some("Docs".into()))
        );
    }

    #[test]
    fn same_window_no_transition_keeps_manual_selection() {
        // Title churn re-matching the same id must not clobber the user's
        // rail click.
        assert_eq!(next_selection(false, Some("Gmail"), Some("Gmail"), None), None);
        assert_eq!(next_selection(false, None, None, None), None);
    }

    #[test]
    fn pin_overrides_transitions() {
        assert_eq!(next_selection(false, None, Some("Gmail"), Some("Pin")), None);
        assert_eq!(next_selection(false, Some("Gmail"), None, Some("Pin")), None);
    }
}

/// Emit the appearance (theme + panel opacity) to all windows.
pub fn emit_appearance(app: &AppHandle) {
    let payload = {
        let state = app.state::<AppState>();
        let settings = state.settings.lock().unwrap();
        AppearancePayload {
            theme: settings.appearance.theme.clone(),
            panel_opacity: settings.appearance.panel_opacity,
            font_scale: settings.appearance.font_scale,
        }
    };
    let _ = app.emit("appearance://changed", payload);
}
