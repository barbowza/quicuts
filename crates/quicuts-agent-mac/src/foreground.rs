//! Frontmost-app watching + cache.
//!
//! Two roles, mirroring the Windows agent's `foreground.rs`:
//! 1. a cached snapshot of the frontmost app, read at activation time (the
//!    hold timer fires on another thread, and the overlay steals frontmost
//!    the moment it shows — the cache is "the app that was frontmost
//!    *before* activation");
//! 2. an `NSWorkspaceDidActivateApplicationNotification` observer that
//!    streams `ForegroundChanged` so the app can pre-assemble pages.
//!
//! Identity: `exe_name` carries the **bundle identifier** (macOS's stable
//! app identity — `match_foreground` normalizes it exactly like an exe
//! name), `exe_path` the `.app` bundle path the app-side Info.plist reader
//! consumes. Never window titles (no `title` capability in the slice).

use std::ptr::NonNull;
use std::sync::Mutex;

use objc2_app_kit::{
    NSRunningApplication, NSWorkspace, NSWorkspaceDidActivateApplicationNotification,
};
use objc2_foundation::{NSNotification, NSOperationQueue};
use quicuts_proto::{AgentEvent, ForegroundInfo};

use crate::ipc::EventSink;

static CURRENT: Mutex<Option<ForegroundInfo>> = Mutex::new(None);

/// Bundle id of the Quicuts app itself; its windows must never be reported
/// as the foreground (the panel taking focus would flip the page).
const OWN_BUNDLE_ID: &str = "com.barbowza.quicuts";

fn info_from(app: &NSRunningApplication) -> ForegroundInfo {
    let exe_name = app.bundleIdentifier().map(|s| s.to_string());
    // Prefer the `.app` bundle path — that is what the app-side Info.plist
    // reader consumes. An unbundled process (a `cargo tauri dev` build, a
    // bare binary) has no `bundleURL` at all, so fall back to the executable
    // itself; without that fallback the dev build has *no* identifying field.
    let exe_path = app
        .bundleURL()
        .or_else(|| app.executableURL())
        .and_then(|u| u.path())
        .map(|s| s.to_string());
    ForegroundInfo {
        pid: app.processIdentifier() as u32,
        exe_path,
        exe_name,
        title: None,
    }
}

fn is_own(info: &ForegroundInfo) -> bool {
    // Bundled: the bundle id. Unbundled (dev), there is no bundle id, so
    // match the bare executable path `info_from` fell back to.
    info.exe_name.as_deref() == Some(OWN_BUNDLE_ID)
        || info
            .exe_path
            .as_deref()
            .is_some_and(|p| p.ends_with("/quicuts") || p.ends_with("/Quicuts.app"))
}

/// Re-resolve the frontmost app and update the cache. Main thread only
/// (NSWorkspace). Returns None for our own windows, keeping the cache on
/// the user's real app.
fn refresh() -> Option<ForegroundInfo> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let info = info_from(&app);
    // Ours, or carrying nothing `match_foreground` could ever key on: either
    // way, caching it would replace the user's real app with a panel that
    // shows the unsupported-app placeholder. Leave the cache alone.
    if is_own(&info) || (info.exe_name.is_none() && info.exe_path.is_none()) {
        return None;
    }
    *CURRENT.lock().unwrap() = Some(info.clone());
    Some(info)
}

/// Snapshot of the last real foreground app; safe from any thread.
pub fn current() -> Option<ForegroundInfo> {
    CURRENT.lock().unwrap().clone()
}

/// Take the initial snapshot and install the activation observer. Main
/// thread, before the run loop starts; the observer token lives for the
/// process lifetime.
pub fn install(sink: EventSink) {
    refresh();
    let block = block2::RcBlock::new(move |_n: NonNull<NSNotification>| {
        if let Some(info) = refresh() {
            sink.send(AgentEvent::ForegroundChanged { foreground: info });
        }
    });
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let token = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidActivateApplicationNotification),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &block,
        )
    };
    std::mem::forget(token);
}
