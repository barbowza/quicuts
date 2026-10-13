//! macOS: turn the overlay into a non-activating panel (ADR 0006).
//!
//! An ordinary `NSWindow` can only take keystrokes once its app is active,
//! so showing the overlay used to activate Quicuts: the menu bar switched
//! to it, and the user's app lost active status (and, for Safari/Chrome,
//! its focused-field caret) on every show. An `NSPanel` with
//! `NSWindowStyleMaskNonactivatingPanel` can become key *without*
//! activating its app. That is what Spotlight and Raycast do.
//!
//! Tauri v2 doesn't expose panels, so the overlay's already-created
//! `TaoWindow` is re-classed in place to an `NSPanel` subclass
//! (`object_setClass`), the technique `tauri-nspanel` uses. Swapping the
//! class of a live object is only sound when the new class has the same
//! instance layout, so this checks it at runtime and leaves the window
//! alone if anything differs. tao keeps a `focusable` ivar on `TaoWindow`
//! and reads it from `canBecomeKeyWindow` and `set_focusable`, so the panel
//! class declares the same ivar at the same offset.
//!
//! If the check fails (a tao upgrade changed `TaoWindow`), the overlay
//! still works; it just activates Quicuts again, as before. The log says
//! which check failed.

use objc2::ffi::{object_getClass, object_setClass};
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{sel, ClassType};
use objc2_app_kit::{NSPanel, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask};

const FOCUSABLE: &std::ffi::CStr = c"focusable";

/// Set once the swap succeeded; until then the overlay is an ordinary
/// activating window and must still be focused the ordinary way.
static INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the overlay is a non-activating panel (see `overlay::focus`).
pub fn installed() -> bool {
    INSTALLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Re-class the overlay window as a non-activating panel. Must run on the
/// main thread (Tauri's `setup` does) and before the window is first shown:
/// the window server reads the non-activating flag when it orders the
/// window in.
pub fn install(window: &tauri::WebviewWindow) {
    let Ok(ptr) = window.ns_window() else {
        log::warn!("panel: no NSWindow for the overlay; leaving it activating");
        return;
    };
    let obj = ptr as *mut AnyObject;
    // SAFETY: `ns_window` is the live NSWindow tao created for this
    // webview; we are on the main thread, and nothing else touches it here.
    unsafe {
        let current = &*object_getClass(obj);
        let Some(panel) = panel_class() else {
            log::warn!("panel: could not define QuicutsPanel; leaving the overlay activating");
            return;
        };
        if let Err(why) = compatible(current, panel) {
            log::warn!("panel: {} -> QuicutsPanel unsafe ({why}); leaving the overlay activating", current.name().to_string_lossy());
            return;
        }
        if current.name() != c"TaoWindow" {
            log::info!(
                "panel: replacing {} (observed keys: {})",
                current.name().to_string_lossy(),
                observed_keys(obj)
            );
        }
        object_setClass(obj, panel);

        let win: &NSWindow = &*(obj as *const NSWindow);
        win.setStyleMask(win.styleMask() | NSWindowStyleMask::NonactivatingPanel);
        // A panel hides when its app deactivates by default. Quicuts is
        // never the active app now, so that would hide it immediately.
        win.setHidesOnDeactivate(false);
        // Show on whichever Space is current, and over full-screen apps,
        // like the Windows panel shows over everything.
        win.setCollectionBehavior(
            win.collectionBehavior()
                | NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        let panel: &NSPanel = &*(obj as *const NSPanel);
        panel.setFloatingPanel(true);
        // Take keys on show, so the filter box gets typing straight away.
        panel.setBecomesKeyOnlyIfNeeded(false);
    }
    INSTALLED.store(true, std::sync::atomic::Ordering::Relaxed);
    log::info!("panel: overlay is a non-activating NSPanel");
}

/// `NSPanel` subclass carrying tao's `focusable` ivar, so code that still
/// thinks the object is a `TaoWindow` reads and writes the right bytes.
fn panel_class() -> Option<&'static AnyClass> {
    static CLASS: std::sync::OnceLock<Option<&'static AnyClass>> = std::sync::OnceLock::new();
    *CLASS.get_or_init(|| {
        let mut builder = ClassBuilder::new(c"QuicutsPanel", NSPanel::class())?;
        builder.add_ivar::<Bool>(FOCUSABLE);
        extern "C-unwind" fn can_become_key(_this: &AnyObject, _sel: Sel) -> Bool {
            Bool::YES
        }
        extern "C-unwind" fn can_become_main(_this: &AnyObject, _sel: Sel) -> Bool {
            // A panel is never the main window; keeps the user's window main.
            Bool::NO
        }
        // SAFETY: the signatures match the selectors' (`-(BOOL)…`).
        unsafe {
            builder.add_method(
                sel!(canBecomeKeyWindow),
                can_become_key as extern "C-unwind" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(canBecomeMainWindow),
                can_become_main as extern "C-unwind" fn(_, _) -> _,
            );
        }
        Some(builder.register())
    })
}

/// tao's class (or AppKit's KVO subclass of it), the same instance size, and
/// `focusable` at the same offset. Without the last two, `object_setClass`
/// would let one class's methods read the other's bytes.
///
/// The overlay is already key-value observed by the time `setup` runs (seen
/// on device: `NSKVONotifying_TaoWindow`; not by tao or wry, which use
/// notifications). Swapping that isa drops only KVO's auto-notifying setter
/// overrides; observation info lives in a side table, so manual
/// will/didChange notifications still reach observers. The first version of
/// this file had no class-name check and passed every on-device check,
/// presumably swapping this same class. Any other re-classing (a different
/// dynamic subclass) is refused.
fn compatible(current: &AnyClass, panel: &AnyClass) -> Result<(), String> {
    let tao = current.name() == c"TaoWindow";
    let kvo_of_tao = current.name() == c"NSKVONotifying_TaoWindow"
        && current.superclass().map(|c| c.name()) == Some(c"TaoWindow");
    if !(tao || kvo_of_tao) {
        return Err("neither TaoWindow nor its KVO subclass".into());
    }
    if current.instance_size() != panel.instance_size() {
        return Err(format!(
            "instance size {} vs {}",
            current.instance_size(),
            panel.instance_size()
        ));
    }
    match (current.instance_variable(FOCUSABLE), panel.instance_variable(FOCUSABLE)) {
        (Some(a), Some(b)) if a.offset() == b.offset() => Ok(()),
        (None, _) => Err("no `focusable` ivar on the current class".into()),
        _ => Err("`focusable` ivar at a different offset".into()),
    }
}

/// `observationInfo`'s description, for the log: which keys something
/// observes on the window whose KVO class the swap replaces.
unsafe fn observed_keys(obj: *mut AnyObject) -> String {
    use objc2::msg_send;
    let info: *mut AnyObject = msg_send![obj, observationInfo];
    if info.is_null() {
        return "none".into();
    }
    let desc: *mut AnyObject = msg_send![info, description];
    if desc.is_null() {
        return "?".into();
    }
    let utf8: *const std::ffi::c_char = msg_send![desc, UTF8String];
    if utf8.is_null() {
        return "?".into();
    }
    std::ffi::CStr::from_ptr(utf8).to_string_lossy().split_whitespace().collect::<Vec<_>>().join(" ")
}
