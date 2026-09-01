//! Reading another app's focused-window title through the Accessibility API.
//!
//! macOS has no equivalent of `GetWindowTextW`: a window's title belongs to
//! the owning process, and the only supported way to read it from outside is
//! AX. Two routes exist and only one is acceptable here:
//!
//! - **AX** (`AXUIElementCopyAttributeValue`) needs the *Accessibility*
//!   grant — the very grant the active `CGEventTap` already requires, so
//!   titles cost no additional TCC prompt. Verified on real hardware: with
//!   only the terminal enabled in Accessibility, focused-window titles read
//!   back for every app that has a window (apps without one return
//!   `kAXErrorNoValue`, not a permission error).
//! - **`CGWindowListCopyWindowInfo`**'s `kCGWindowName` would need *Screen
//!   Recording* — a second, far broader, and much scarier prompt for the
//!   same string. Rejected; see ADR 0006.
//!
//! Every call here is synchronous cross-process IPC to the target app, so
//! it must never run on the agent's main run loop — a hung app would stall
//! the event tap into `TapDisabledByTimeout`. `foreground.rs` calls this
//! only from its poll thread, and the messaging timeout below bounds even
//! that.

use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};

/// `AXError`; only "no error" matters to us.
const AX_SUCCESS: i32 = 0;

/// Cap on how long one AX round trip may block the poll thread. Beach-balled
/// apps do not answer, and the default timeout is several seconds — long
/// enough for a wedged browser to stall every later poll.
const MESSAGING_TIMEOUT_SECS: f32 = 0.25;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    /// Returns a +1 `AXUIElementRef` (Create rule) — the caller releases it.
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    /// Writes a +1 reference into `value` (Copy rule) on success.
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, timeout: f32) -> i32;
}

/// Copy one AX attribute, taking ownership of the reference the Copy rule
/// hands back. `None` for any error — an unset attribute, an unsupported
/// one, a window that vanished mid-read, or a timeout — all of which are
/// ordinary here and none of which deserve a log line on a 200ms poll.
fn copy_attr(element: CFTypeRef, attribute: &str) -> Option<CFType> {
    if element.is_null() {
        return None;
    }
    let name = CFString::new(attribute);
    let mut out: CFTypeRef = std::ptr::null();
    let err = unsafe {
        AXUIElementCopyAttributeValue(element, name.as_concrete_TypeRef(), &mut out)
    };
    if err != AX_SUCCESS || out.is_null() {
        return None;
    }
    Some(unsafe { CFType::wrap_under_create_rule(out) })
}

/// Title of `pid`'s focused window, or `None` when the app has no window,
/// does not implement AX, or does not answer in time.
///
/// Falls back from `AXFocusedWindow` to `AXMainWindow`: an app that is not
/// frontmost has no *focused* window, and the poll can race a foreground
/// switch. The attribute names are built as literals rather than linked
/// from the `kAX*` globals — they are stable API constants, and this keeps
/// the extern surface to three functions.
pub fn focused_window_title(pid: u32) -> Option<String> {
    let raw = unsafe { AXUIElementCreateApplication(pid as i32) };
    if raw.is_null() {
        return None;
    }
    // Own the +1 from the Create rule; released when `app` drops.
    let app = unsafe { CFType::wrap_under_create_rule(raw) };
    // Documented to apply to every message sent to this application, but
    // set it on the window element too — the cost is one call and a stalled
    // poll thread is the failure we are buying out of.
    unsafe { AXUIElementSetMessagingTimeout(app.as_CFTypeRef(), MESSAGING_TIMEOUT_SECS) };

    let window = copy_attr(app.as_CFTypeRef(), "AXFocusedWindow")
        .or_else(|| copy_attr(app.as_CFTypeRef(), "AXMainWindow"))?;
    unsafe { AXUIElementSetMessagingTimeout(window.as_CFTypeRef(), MESSAGING_TIMEOUT_SECS) };

    let title = copy_attr(window.as_CFTypeRef(), "AXTitle")?
        .downcast_into::<CFString>()?
        .to_string();
    // An untitled window reports "", which is not a signal worth reporting
    // and would otherwise churn against a real title.
    (!title.is_empty()).then_some(title)
}
