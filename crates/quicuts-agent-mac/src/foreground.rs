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
//! consumes.
//!
//! Window titles (role 3, ADR 0003) are read separately, by the poll thread
//! below, and only while the experimental title-detection toggle is on —
//! see `set_title_watch`.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use objc2_app_kit::{
    NSRunningApplication, NSWorkspace, NSWorkspaceDidActivateApplicationNotification,
};
use objc2_foundation::{NSNotification, NSOperationQueue};
use quicuts_proto::{AgentEvent, ForegroundInfo};

use crate::axtitle;
use crate::ipc::EventSink;

static CURRENT: Mutex<Option<ForegroundInfo>> = Mutex::new(None);

/// Bumped every time `refresh` installs a new app in `CURRENT`. The title
/// poller keys its "already reported this" state on it, so switching away
/// from an app and back re-reports the title even when the string is
/// identical — otherwise the switch's own `title: None` report would leave
/// the panel on the host browser with nothing to correct it.
static FG_GEN: AtomicU64 = AtomicU64::new(0);

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
        // Deliberately not read here. This runs on the main thread (the
        // NSWorkspace observer), and an AX title read is synchronous IPC
        // into an app that has just been activated and is therefore quite
        // likely busy. Blocking the main run loop blocks the event tap
        // callback, which is exactly how a tap earns `TapDisabledByTimeout`.
        // The poll thread fills the title in a beat later, via a second
        // `ForegroundChanged` — the same shape the Windows title watcher
        // emits after its NAMECHANGE debounce.
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
    {
        // Bump under the lock so `current_with_gen` can read the pair
        // consistently; a reader that saw the new app with the old
        // generation would attribute the previous app's title to it.
        let mut cur = CURRENT.lock().unwrap();
        *cur = Some(info.clone());
        FG_GEN.fetch_add(1, Ordering::Release);
    }
    Some(info)
}

/// Snapshot of the last real foreground app; safe from any thread.
pub fn current() -> Option<ForegroundInfo> {
    CURRENT.lock().unwrap().clone()
}

/// The cached app together with the generation it belongs to, read as one
/// consistent pair (see `refresh`). The title poller needs both: the app to
/// report and the generation to detect that the foreground moved under it.
fn current_with_gen() -> Option<(u64, ForegroundInfo)> {
    let cur = CURRENT.lock().unwrap();
    let gen = FG_GEN.load(Ordering::Acquire);
    cur.clone().map(|info| (gen, info))
}

/// Store a freshly-polled title on the cached app, so the snapshot taken at
/// activation time carries it too. A no-op if the foreground moved on since
/// the poll started (the generation guard) — the cache must never end up
/// describing one app with another's title.
fn store_title(gen: u64, title: Option<String>) {
    let mut cur = CURRENT.lock().unwrap();
    if FG_GEN.load(Ordering::Acquire) != gen {
        return;
    }
    if let Some(info) = cur.as_mut() {
        info.title = title;
    }
}

/// Experimental title watching (ADR 0003), off by default and driven by
/// `Configure.title_events_enabled`.
struct TitleWatch {
    enabled: Mutex<bool>,
    wake: Condvar,
}

static TITLE_WATCH: TitleWatch =
    TitleWatch { enabled: Mutex::new(false), wake: Condvar::new() };

/// How often the poll thread re-reads the frontmost window's title, and
/// therefore also the debounce: a title must survive one full interval
/// unchanged before it is reported. That reproduces the Windows agent's
/// 200ms `EVENT_OBJECT_NAMECHANGE` debounce (`hook.rs`) and for the same
/// reason — Gmail rewrites its title several times per navigation as the
/// unread count and loading state settle.
///
/// Polling, rather than the `AXObserver`/`kAXTitleChangedNotification`
/// route: an observer is per-process, so it would have to be torn down and
/// rebuilt on every app switch, it needs its run-loop source on a thread
/// that is not the tap's, and apps vary in how faithfully they post the
/// notification. One bounded AX read every 200ms — only while the toggle is
/// on — costs less than that machinery and cannot wedge the tap.
const TITLE_POLL_MS: u64 = 200;

/// Turn title watching on or off. Called from the command pump on every
/// `Configure`; cheap and idempotent.
pub fn set_title_watch(enabled: bool) {
    let mut on = TITLE_WATCH.enabled.lock().unwrap_or_else(|e| e.into_inner());
    if *on == enabled {
        return;
    }
    *on = enabled;
    drop(on);
    eprintln!("title watch: {}", if enabled { "enabled" } else { "disabled" });
    TITLE_WATCH.wake.notify_all();
}

/// Poll loop. Parks on the condvar while the toggle is off, so a Quicuts
/// with the experimental feature disabled issues no AX traffic at all and
/// wakes no timer.
fn title_poll_loop(sink: EventSink) {
    // Last (generation, title) sampled: a sample must repeat before it is
    // reported, which is the debounce.
    let mut last_seen: Option<(u64, Option<String>)> = None;
    // Title already reported for `sent_gen`. Seeded to `None` for each new
    // generation because the foreground switch that opened it *already*
    // reported `title: None` — so "still no title" is never news, while a
    // title appearing, changing, or vanishing all are.
    let mut sent_gen = u64::MAX;
    let mut sent_title: Option<String> = None;
    loop {
        {
            let mut on = TITLE_WATCH.enabled.lock().unwrap_or_else(|e| e.into_inner());
            while !*on {
                last_seen = None;
                sent_gen = u64::MAX;
                sent_title = None;
                on = TITLE_WATCH
                    .wake
                    .wait(on)
                    .unwrap_or_else(|e| e.into_inner());
            }
        }
        std::thread::sleep(Duration::from_millis(TITLE_POLL_MS));
        if !*TITLE_WATCH.enabled.lock().unwrap_or_else(|e| e.into_inner()) {
            continue;
        }
        let Some((gen, mut info)) = current_with_gen() else {
            last_seen = None;
            continue;
        };
        // The AX call below blocks (bounded, but still). Anything sampled
        // here is tagged with the generation it was taken under, so a
        // foreground switch mid-read invalidates the sample rather than
        // silently attributing one app's title to another.
        let sample = (gen, axtitle::focused_window_title(info.pid));
        if last_seen.as_ref() != Some(&sample) {
            last_seen = Some(sample);
            continue; // still settling
        }
        if sent_gen != gen {
            sent_gen = gen;
            sent_title = None;
        }
        if sent_title == sample.1 {
            continue; // unchanged since the last report
        }
        // Re-check before reporting: `info` describes the app as of the
        // start of this poll, so emitting it after a switch would tell the
        // app that the *old* app is frontmost, clobbering the newer
        // ForegroundChanged the NSWorkspace observer has already sent.
        if FG_GEN.load(Ordering::Acquire) != gen {
            last_seen = None;
            continue;
        }
        sent_title = sample.1.clone();
        info.title = sample.1;
        store_title(gen, info.title.clone());
        sink.send(AgentEvent::ForegroundChanged { foreground: info });
    }
}

/// Take the initial snapshot and install the activation observer. Main
/// thread, before the run loop starts; the observer token lives for the
/// process lifetime.
pub fn install(sink: EventSink) {
    refresh();
    {
        let sink = sink.clone();
        std::thread::spawn(move || title_poll_loop(sink));
    }
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
