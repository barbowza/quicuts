//! Overlay + badge window control: positioning, show/hide, state push.

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize};

use crate::app_state::AppState;

const PANEL_WIDTH: f64 = 586.0;

/// Recompute the overlay view model and push it to the overlay window.
pub fn push_state(app: &AppHandle) {
    let state = app.state::<AppState>();
    let payload = {
        let fg = state.foreground.lock().unwrap();
        let selected = state.selected.lock().unwrap().clone();
        let pinned = state.pinned_app.lock().unwrap().clone();
        let title_matched = state.title_matched.lock().unwrap().clone();
        let (mode, activation, host_classes) = {
            let s = state.settings.lock().unwrap();
            (
                s.combo_display_mode.clone(),
                s.activation.clone(),
                quicuts_manifest::HostClasses::with_extensions(&s.extra_browser_exes),
            )
        };
        let pins = state.pins.lock().unwrap();
        let customs = state.customs.lock().unwrap();
        state.engine.lock().unwrap().build_state(
            fg.as_ref(),
            &pins,
            &customs,
            selected,
            pinned.as_deref(),
            title_matched.as_deref(),
            &host_classes,
            mode,
            activation,
        )
    };
    // Remember the resolved selection.
    *state.selected.lock().unwrap() = payload.selected.clone();
    let _ = app.emit("overlay://state", &payload);
    sync_badges(app);
}

/// Arm or disarm the taskbar badges: whenever the overlay is up (and the
/// setting is on), ask the agent for fresh button geometry (the reply lands
/// in `update_badges`); otherwise hide.
fn sync_badges(app: &AppHandle) {
    let state = app.state::<AppState>();
    let visible = *state.overlay_visible.lock().unwrap();
    let enabled = state.settings.lock().unwrap().taskbar_badges;
    let armed = visible && enabled;
    if visible {
        log::info!("badges: armed={armed} (setting={enabled})");
    }
    if armed {
        crate::agent::send(app, &quicuts_proto::AgentCommand::QueryTaskbar);
    } else if let Some(badges) = app.get_webview_window("badges") {
        let _ = badges.hide();
    }
}

pub fn show(app: &AppHandle) {
    // The hidden webviews may have loaded before AppState was managed, so
    // their mount-time appearance fetch can miss; re-push on every show.
    crate::agent::emit_appearance(app);
    // Mark visible before the push so it arms the taskbar badges.
    *app.state::<AppState>().overlay_visible.lock().unwrap() = true;
    push_state(app);
    let Some(window) = app.get_webview_window("overlay") else {
        return;
    };
    let edge = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .appearance
        .panel_edge
        .clone();

    position_panel(app, &window, &edge);
    // Re-assert topmost: it is dropped while a tray flyout is open and a
    // missed close event must not leave a freshly shown panel non-topmost.
    let _ = window.set_always_on_top(true);
    let _ = window.show();
    let _ = window.set_focus();
}

/// Esc pressed (via the agent's `Dismissed` or the webview): with the
/// two-step option on and text in the filter box, the first Esc clears the
/// filter and keeps the panel up; otherwise hide. Lives here (not in the
/// webview) because the agent swallows Esc while the panel is focused, so
/// the DOM never sees it.
pub fn dismiss(app: &AppHandle) {
    let state = app.state::<AppState>();
    // A customization dialog or key-capture box owns Esc before the filter:
    // peel one modal layer and let the webview close it. Optimistically step
    // the stored value down (capture -> dialog -> none) so a missed event
    // can't trap Esc; the webview re-reports the real state.
    {
        let mut modal = state.overlay_modal.lock().unwrap();
        if *modal != "none" {
            *modal = if *modal == "capture" { "dialog".into() } else { "none".into() };
            drop(modal);
            let _ = app.emit_to("overlay", "overlay://modal-esc", ());
            return;
        }
    }
    let two_step = state.settings.lock().unwrap().esc_clears_filter;
    let filter_active = *state.filter_active.lock().unwrap();
    if two_step && filter_active {
        // Optimistically mark it cleared so a second Esc always closes even
        // if the webview misses the event; the webview also reports back.
        *state.filter_active.lock().unwrap() = false;
        let _ = app.emit_to("overlay", "overlay://clear-filter", ());
        return;
    }
    hide(app);
    crate::agent::send(
        app,
        &quicuts_proto::AgentCommand::SetOverlayVisible { visible: false },
    );
}

/// While a shell tray flyout is open, slot the panel/badges directly
/// *beneath the flyout window* in the z-order; restore topmost on close.
/// The Win11 overflow flyout is not WS_EX_TOPMOST, so merely dropping
/// always-on-top (= HWND_NOTOPMOST) would re-insert the panel above all
/// non-topmost windows — covering the flyout again. Inserting after the
/// flyout's own handle puts the flyout on top while the panel stays above
/// the user's app windows. A missed close event can't wedge the panel low:
/// `show` re-asserts topmost.
pub fn duck_under_flyout(app: &AppHandle, open: bool, flyout: Option<u64>) {
    for label in ["overlay", "badges"] {
        let Some(w) = app.get_webview_window(label) else {
            continue;
        };
        if !open {
            let _ = w.set_always_on_top(true);
            continue;
        }
        #[cfg(windows)]
        if let (Some(fly), Ok(hwnd)) = (flyout, w.hwnd()) {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{
                SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            };
            // Moving below a non-topmost window also clears the panel's
            // topmost status until `set_always_on_top(true)` restores it.
            let done = unsafe {
                SetWindowPos(
                    HWND(hwnd.0 as isize as *mut _),
                    Some(HWND(fly as usize as *mut _)),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
            if done.is_ok() {
                continue;
            }
        }
        // No handle (or the flyout died mid-flight): best effort.
        let _ = w.set_always_on_top(false);
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
    if let Some(badges) = app.get_webview_window("badges") {
        let _ = badges.hide();
    }
    *app.state::<AppState>().overlay_visible.lock().unwrap() = false;
}

/// Snap the panel to the left/right edge of the monitor under the cursor.
/// Uses the monitor's work area (not full bounds) so the panel never covers
/// the taskbar, whichever edge it's docked to.
fn position_panel(app: &AppHandle, window: &tauri::WebviewWindow, edge: &str) {
    let monitor = monitor_under_cursor(app, window);
    let (mx, my, mw, mh, scale) = match monitor {
        Some(m) => {
            let wa = m.work_area();
            (
                wa.position.x,
                wa.position.y,
                wa.size.width,
                wa.size.height,
                m.scale_factor(),
            )
        }
        None => (0, 0, 1920, 1080, 1.0),
    };
    let panel_px = (PANEL_WIDTH * scale) as u32;
    let _ = window.set_size(PhysicalSize::new(panel_px, mh));
    let x = if edge == "left" { mx } else { mx + mw as i32 - panel_px as i32 };
    let _ = window.set_position(PhysicalPosition::new(x, my));
    // Keep the logical width stable regardless of DPI for the web layout.
    let _ = window.set_size(LogicalSize::new(PANEL_WIDTH, mh as f64 / scale));
}

/// Monitor whose bounds contain the given physical point.
fn monitor_at(window: &tauri::WebviewWindow, x: i32, y: i32) -> Option<tauri::Monitor> {
    for m in window.available_monitors().ok()? {
        let p = m.position();
        let s = m.size();
        if x >= p.x && x < p.x + s.width as i32 && y >= p.y && y < p.y + s.height as i32 {
            return Some(m.clone());
        }
    }
    None
}

fn monitor_under_cursor(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
) -> Option<tauri::Monitor> {
    let cursor = app.cursor_position().ok()?;
    let monitors = window.available_monitors().ok()?;
    for m in &monitors {
        let p = m.position();
        let s = m.size();
        let (x0, y0) = (p.x as f64, p.y as f64);
        let (x1, y1) = (x0 + s.width as f64, y0 + s.height as f64);
        if cursor.x >= x0 && cursor.x < x1 && cursor.y >= y0 && cursor.y < y1 {
            return Some(m.clone());
        }
    }
    window.primary_monitor().ok().flatten()
}

#[derive(Serialize, Clone)]
struct Badge {
    label: String,
    x: f64,
    y: f64,
}

/// Thickness (logical px) of the badge row hovering off the taskbar's edge.
/// Generous so the chips still fit when WebView2 zooms the page for Windows
/// text scaling (chips grow with it).
const BADGE_STRIP: f64 = 40.0;

/// Position the badge window as a thin strip along the taskbar's inner edge
/// (above a bottom bar), one chip centered over each button.
pub fn update_badges(app: &AppHandle, snap: &quicuts_proto::TaskbarSnapshot) {
    let state = app.state::<AppState>();
    let show_badges = state.settings.lock().unwrap().taskbar_badges;
    // A snapshot can arrive after the overlay was hidden (query in flight
    // when the user released); never surface badges over a hidden panel.
    let overlay_up = *state.overlay_visible.lock().unwrap();
    let Some(badges_win) = app.get_webview_window("badges") else {
        return;
    };
    if !show_badges || !overlay_up || snap.buttons.is_empty() {
        let _ = badges_win.hide();
        return;
    }
    let tb = snap.taskbar_rect;
    // Scale of the monitor holding the taskbar — the window's own
    // scale_factor() reads 1.0 while it is hidden.
    let scale = monitor_at(&badges_win, tb.x + tb.w / 2, tb.y + tb.h / 2)
        .map(|m| m.scale_factor())
        .or_else(|| badges_win.scale_factor().ok())
        .unwrap_or(1.0);
    log::info!("badges: scale={scale}, taskbar rect {tb:?}");
    let strip = (BADGE_STRIP * scale).round() as i32;
    use quicuts_proto::Edge;
    let (wx, wy, ww, wh) = match snap.edge {
        Edge::Top => (tb.x, tb.y + tb.h, tb.w, strip),
        Edge::Left => (tb.x + tb.w, tb.y, strip, tb.h),
        Edge::Right => (tb.x - strip, tb.y, strip, tb.h),
        _ => (tb.x, tb.y - strip, tb.w, strip),
    };
    let horizontal = ww >= wh;
    let _ = badges_win.set_position(PhysicalPosition::new(wx, wy));
    let _ = badges_win.set_size(PhysicalSize::new(ww.max(1) as u32, wh.max(1) as u32));
    // Realized vs requested geometry exposes any coordinate-space mismatch
    // (DPI virtualization) between the agent's rects and this window.
    if let (Ok(pos), Ok(size)) = (badges_win.outer_position(), badges_win.inner_size()) {
        log::info!(
            "badges: window at {},{} {}x{} (requested {wx},{wy} {ww}x{wh})",
            pos.x, pos.y, size.width, size.height
        );
    }
    #[cfg(windows)]
    {
        let _ = badges_win.set_ignore_cursor_events(true);
    }
    let chips: Vec<Badge> = snap
        .buttons
        .iter()
        .map(|b| {
            // Centered on the button along the bar, centered in the strip
            // across it. PHYSICAL window-local px: the webview divides by its
            // own devicePixelRatio, which (unlike the monitor scale here)
            // also reflects WebView2's zoom for Windows text scaling.
            let along = if horizontal {
                (b.rect.x - wx) as f64 + b.rect.w as f64 / 2.0
            } else {
                (b.rect.y - wy) as f64 + b.rect.h as f64 / 2.0
            };
            let across = strip as f64 / 2.0;
            Badge {
                label: if b.index >= 10 { "0".into() } else { b.index.to_string() },
                x: if horizontal { along } else { across },
                y: if horizontal { across } else { along },
            }
        })
        .collect();
    let _ = badges_win.show();
    crate::agent::emit_appearance(app);
    let _ = app.emit_to("badges", "badges://positions", chips);
}
