//! Foreground-window detection + change notifications.
//!
//! Two roles:
//! 1. `current_foreground()` — a synchronous snapshot, taken at activation
//!    time (before the overlay steals focus) and embedded in the activation
//!    event, so the app keys the page off the user's real app.
//! 2. a `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` watcher that streams
//!    `ForegroundChanged` so the app can pre-assemble pages ahead of time.

use quicuts_proto::ForegroundInfo;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, MAX_PATH};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowTextW,
    GetWindowThreadProcessId, GWL_EXSTYLE, WS_EX_TOPMOST,
};

/// Exe names that are part of Quicuts' own UI stack; never report them as
/// the foreground. `msedgewebview2.exe` only ever owns WebView2 child
/// surfaces (find bar, dropdowns, devtools) — top-level windows of other
/// WebView2 apps belong to their host exe — so filtering it can't hide a
/// real app, but without the filter Ctrl+F's find bar replaces the page.
const OWN_EXES: [&str; 2] = ["quicuts.exe", "msedgewebview2.exe"];

/// Transient shell windows that briefly take the foreground during Alt+Tab /
/// Task View. They belong to explorer.exe, so reporting them flips the page
/// to "File Explorer" mid-switch; skip them and the previous app sticks.
/// Real File Explorer windows are `CabinetWClass` and still get through.
const TRANSIENT_SHELL_CLASSES: [&str; 5] = [
    "ForegroundStaging",              // Alt+Tab staging window
    "MultitaskingViewFrame",          // Win10 Alt+Tab / Task View
    "XamlExplorerHostIslandWindow",   // Win11 Alt+Tab host
    "TaskSwitcherWnd",                // legacy Alt+Tab
    "TaskSwitcherOverlayWnd",
];

/// The tray "show hidden icons" overflow flyout. Unlike the taskbar's own
/// menus (which live in a privileged shell z-band), this window sits in the
/// ordinary topmost band, so an always-on-top overlay covers it.
const TRAY_FLYOUT_CLASSES: [&str; 2] = [
    "TopLevelWindowForOverflowXamlIsland", // Win11
    "NotifyIconOverflowWindow",            // Win10
];

fn class_name(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    if n <= 0 {
        None
    } else {
        Some(String::from_utf16_lossy(&buf[..n as usize]))
    }
}

pub fn is_transient_shell(hwnd: HWND) -> bool {
    let Some(class) = class_name(hwnd) else {
        return false;
    };
    TRANSIENT_SHELL_CLASSES
        .iter()
        .any(|c| class.eq_ignore_ascii_case(c))
}

/// Shell windows the generic flyout heuristic must never match: the taskbar
/// itself and the desktop. They are shell-owned (and the taskbar is topmost)
/// but taking the foreground on them does not mean a popup is covering us.
const SHELL_SURFACE_CLASSES: [&str; 4] =
    ["Shell_TrayWnd", "Shell_SecondaryTrayWnd", "Progman", "WorkerW"];

/// Shell processes that host tray/taskbar popups.
const SHELL_POPUP_EXES: [&str; 2] = ["explorer.exe", "shellexperiencehost.exe"];

pub fn is_tray_flyout(hwnd: HWND) -> bool {
    let Some(class) = class_name(hwnd) else {
        return false;
    };
    if TRAY_FLYOUT_CLASSES.iter().any(|c| class.eq_ignore_ascii_case(c)) {
        return true;
    }
    // Generic fallback — the flyout class name varies across Windows builds:
    // a *topmost* shell-owned window that is not the taskbar/desktop is a
    // shell popup sharing our z-band, so the overlay must duck under it.
    // Windows in the privileged shell bands (Start, taskbar context menus)
    // beat the overlay anyway; ducking for them too is harmless.
    if SHELL_SURFACE_CLASSES.iter().any(|c| class.eq_ignore_ascii_case(c)) {
        return false;
    }
    let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    if (ex as u32) & WS_EX_TOPMOST.0 == 0 {
        return false;
    }
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return false;
    }
    let (_, exe) = exe_for_pid(pid);
    exe.map(|e| SHELL_POPUP_EXES.iter().any(|s| e.eq_ignore_ascii_case(s)))
        .unwrap_or(false)
}

fn info_for_hwnd(hwnd: HWND) -> Option<ForegroundInfo> {
    if hwnd.0.is_null() {
        return None;
    }
    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }

    let (mut exe_path, mut exe_name) = exe_for_pid(pid);
    // Frame-hosted UWP apps (Calculator, Media Player, ...): the foreground
    // window belongs to ApplicationFrameHost.exe; the real app owns a
    // CoreWindow child inside the frame. Report the app, not the frame.
    if exe_name
        .as_deref()
        .is_some_and(|n| n.eq_ignore_ascii_case("ApplicationFrameHost.exe"))
    {
        if let Some(app_pid) = uwp_child_pid(hwnd, pid) {
            let (p, n) = exe_for_pid(app_pid);
            if n.is_some() {
                pid = app_pid;
                exe_path = p;
                exe_name = n;
            }
        }
    }
    let title = window_title(hwnd);
    Some(ForegroundInfo { pid, exe_path, exe_name, title })
}

/// Pid of the UWP app hosted inside an ApplicationFrameHost window: the
/// first `Windows.UI.Core.CoreWindow` child owned by another process.
/// None if the frame is empty (app suspended, or the CoreWindow not yet
/// reparented right after launch — callers fall back to the frame host).
fn uwp_child_pid(frame: HWND, frame_pid: u32) -> Option<u32> {
    struct Ctx {
        frame_pid: u32,
        found: u32,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid != 0
            && pid != ctx.frame_pid
            && class_name(hwnd).is_some_and(|c| c == "Windows.UI.Core.CoreWindow")
        {
            ctx.found = pid;
            return BOOL(0); // stop enumerating
        }
        BOOL(1)
    }
    let mut ctx = Ctx { frame_pid, found: 0 };
    unsafe {
        let _ = EnumChildWindows(
            Some(frame),
            Some(visit),
            LPARAM(&mut ctx as *mut Ctx as isize),
        );
    }
    (ctx.found != 0).then_some(ctx.found)
}

fn exe_for_pid(pid: u32) -> (Option<String>, Option<String>) {
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(_) => return (None, None),
        };
        let mut buf = [0u16; MAX_PATH as usize];
        let mut len = buf.len() as u32;
        let path = if QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok()
        {
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        } else {
            None
        };
        let _ = windows::Win32::Foundation::CloseHandle(handle);
        let name = path.as_ref().and_then(|p| {
            p.rsplit(['\\', '/']).next().map(|s| s.to_string())
        });
        (path, name)
    }
}

fn window_title(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 {
        None
    } else {
        Some(String::from_utf16_lossy(&buf[..n as usize]))
    }
}

/// Snapshot of the current foreground window (used at activation time).
pub fn current_foreground() -> Option<ForegroundInfo> {
    let hwnd = unsafe { GetForegroundWindow() };
    if is_transient_shell(hwnd) {
        return None;
    }
    info_for_hwnd(hwnd).filter(|f| !is_own(f))
}

pub fn is_own(info: &ForegroundInfo) -> bool {
    info.exe_name
        .as_deref()
        .map(|n| OWN_EXES.iter().any(|own| n.eq_ignore_ascii_case(own)))
        .unwrap_or(false)
}

/// Convert a raw HWND from the WinEvent callback into a report, skipping our
/// own windows and transient shell (Alt+Tab) windows.
pub fn report_for_hwnd(hwnd: HWND) -> Option<ForegroundInfo> {
    if is_transient_shell(hwnd) {
        return None;
    }
    info_for_hwnd(hwnd).filter(|f| !is_own(f))
}
