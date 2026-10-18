//! Quicuts Windows system agent (sidecar).
//!
//! Deliberately isolated from the main app: this is the only Quicuts binary
//! that installs global hooks. It receives its entire configuration over
//! stdin and reports only semantic transitions over stdout — raw keystrokes
//! never leave this process (see quicuts-proto docs).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod foreground;
#[cfg(windows)]
mod hook;
#[cfg(windows)]
mod ipc;
#[cfg(windows)]
mod state;
#[cfg(windows)]
mod taskbar;

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    set_dpi_aware();
    hook::run()
}

/// Opt this process out of DPI virtualization before any COM/UIA call.
///
/// Without it, UI Automation hands `taskbar.rs` button rects in *logical*
/// px while `ABM_GETTASKBARPOS` stays physical, and `overlay.rs` — which
/// treats both as physical — places every badge at x/scale (issue #60, seen
/// at 125%). Per-monitor v2 is what the Tauri app runs under, so the agent's
/// rects land in the same virtual-screen pixel space the badge window uses.
/// Failure (e.g. awareness already set by a manifest) is logged, not fatal:
/// the keyboard hook and foreground watcher don't depend on it.
#[cfg(windows)]
fn set_dpi_aware() {
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    if let Err(e) =
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
    {
        eprintln!("dpi: SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2) failed: {e}");
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("quicuts-agent-win only runs on Windows");
    std::process::exit(1);
}
