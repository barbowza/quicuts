//! Taskbar geometry reader (Windows-only feature, M4).
//!
//! Edge + band rect come from `SHAppBarMessage(ABM_GETTASKBARPOS)`; per-button
//! rects come from UI Automation over `Shell_TrayWnd`. Everything here is
//! best-effort: any failure yields an empty button list rather than an error,
//! because badges are a nice-to-have and the taskbar UIA surface is unstable
//! across Windows builds.
//!
//! Runs on its own STA thread so COM init is isolated from the message pump.

use quicuts_proto::{Edge, Rect, TaskbarButton, TaskbarSnapshot};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, TreeScope_Descendants, UIA_ButtonControlTypeId,
    UIA_ControlTypePropertyId,
};
use windows::Win32::UI::Shell::{
    SHAppBarMessage, ABM_GETTASKBARPOS, ABE_LEFT, ABE_RIGHT, ABE_TOP, APPBARDATA,
};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

fn appbar() -> Option<(Edge, Rect)> {
    unsafe {
        let mut data = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            ..Default::default()
        };
        if SHAppBarMessage(ABM_GETTASKBARPOS, &mut data) == 0 {
            return None;
        }
        let edge = match data.uEdge {
            e if e == ABE_LEFT => Edge::Left,
            e if e == ABE_RIGHT => Edge::Right,
            e if e == ABE_TOP => Edge::Top,
            _ => Edge::Bottom,
        };
        Some((edge, rect_from(data.rc)))
    }
}

fn rect_from(r: RECT) -> Rect {
    Rect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top }
}

fn button_rects() -> Vec<Rect> {
    unsafe {
        let tray = FindWindowW(PCWSTR(wide("Shell_TrayWnd").as_ptr()), PCWSTR::null());
        let tray = match tray {
            Ok(h) if !h.0.is_null() => h,
            _ => return Vec::new(),
        };
        collect_buttons(tray).unwrap_or_default()
    }
}

/// Win11 task-list (pinned/running app) buttons carry this UIA class; Start,
/// search, widgets and tray icons do not. Win+1..9 targets only these.
const TASK_LIST_CLASS: &str = "Taskbar.TaskListButtonAutomationPeer";

/// System buttons excluded from the fallback path (older taskbars where the
/// class filter matches nothing).
const SYSTEM_IDS: &[&str] = &[
    "StartButton",
    "SearchButton",
    "SearchBoxButton",
    "TaskViewButton",
    "WidgetsButton",
    "ChatButton",
    "CopilotButton",
    "ShowDesktopButton",
];

unsafe fn collect_buttons(tray: HWND) -> windows::core::Result<Vec<Rect>> {
    let automation: IUIAutomation =
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
    let root = automation.ElementFromHandle(tray)?;
    let value = windows::Win32::System::Variant::VARIANT::from(UIA_ButtonControlTypeId.0);
    let condition = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &value)?;
    let found = root.FindAll(TreeScope_Descendants, &condition)?;
    let count = found.Length()?;
    let mut task_list = Vec::new();
    let mut fallback = Vec::new();
    for i in 0..count {
        if let Ok(el) = found.GetElement(i) {
            if let Ok(r) = el.CurrentBoundingRectangle() {
                // Skip zero-area / offscreen elements.
                if r.right <= r.left || r.bottom <= r.top {
                    continue;
                }
                let class = el.CurrentClassName().map(|s| s.to_string()).unwrap_or_default();
                if class == TASK_LIST_CLASS {
                    task_list.push(rect_from(r));
                } else {
                    let id = el
                        .CurrentAutomationId()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    if !SYSTEM_IDS.contains(&id.as_str()) {
                        fallback.push(rect_from(r));
                    }
                }
            }
        }
    }
    Ok(if task_list.is_empty() { fallback } else { task_list })
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Read one taskbar snapshot. Call from a dedicated thread (does COM init).
pub fn read_snapshot() -> Option<TaskbarSnapshot> {
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let did_init = hr.is_ok();
        let result = (|| {
            let (edge, taskbar_rect) = appbar()?;
            let mut rects = button_rects();
            // UIA tree order usually matches visual order, but Win+N goes by
            // on-screen position; make that explicit.
            if matches!(edge, Edge::Top | Edge::Bottom) {
                rects.sort_by_key(|r| r.x);
            } else {
                rects.sort_by_key(|r| r.y);
            }
            let buttons: Vec<TaskbarButton> = rects
                .into_iter()
                .take(9)
                .enumerate()
                .map(|(i, rect)| TaskbarButton { index: (i + 1) as u32, rect })
                .collect();
            Some(TaskbarSnapshot { edge, taskbar_rect, buttons })
        })();
        if did_init {
            CoUninitialize();
        }
        result
    }
}
