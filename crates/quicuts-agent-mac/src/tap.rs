//! CGEventTap adapter: the only unsafe/system half of activation.
//!
//! An *active* session tap (head-insert) is required — passive NSEvent
//! monitors cannot swallow events, and Quicuts must swallow the chord key
//! and Esc-while-visible. The tap callback translates CGKeyCode → the
//! Windows-style VK the protocol speaks, feeds the pure state machine in
//! `activation.rs`, and acts on its decision. It runs on the main-thread
//! run loop and never blocks on IPC (config reads are atomics, event
//! emission is a channel send).
//!
//! The system disables taps whose callbacks stall (`TapDisabledByTimeout`,
//! also seen after sleep/wake): recovery is re-enabling from the callback,
//! plus a watchdog thread for the case where a dead tap gets no callbacks
//! at all.

use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::time::Duration;

use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CGEventTapProxy, CallbackResult, EventField, KeyCode,
};
use quicuts_proto::{caps, AgentCommand, AgentEvent, DismissReason, FatalKind, PROTO_VERSION};

use crate::activation::{Activation, Event, TimerCmd};
use crate::foreground;
use crate::ipc::{self, EventSink};
use crate::state::{self, CONFIG, EXCLUDED};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    /// TCC check for the Accessibility grant an active keyboard tap needs.
    /// Attributed to the *responsible process* (the Quicuts app bundle, or
    /// the terminal in dev) — not this binary.
    fn AXIsProcessTrusted() -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapEnable(tap: core_foundation::mach_port::CFMachPortRef, enable: bool);
    fn CGEventTapIsEnabled(tap: core_foundation::mach_port::CFMachPortRef) -> bool;
}

static ACTIVATION: Mutex<Activation> = Mutex::new(Activation::new());

/// The tap's CFMachPort, for re-enabling from the callback / watchdog. Set
/// once after creation; the port outlives both users (the tap is never
/// dropped).
static TAP_PORT: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

fn reenable_tap() {
    let port = TAP_PORT.load(Ordering::Acquire);
    if !port.is_null() {
        unsafe { CGEventTapEnable(port as _, true) };
    }
}

enum TimerMsg {
    Arm(u32),
    Cancel,
}

/// Hold-timer thread: one pending deadline at most; re-arm replaces it.
/// A stale fire that races a cancel is harmless — the state machine only
/// acts on it in the CmdDown phase.
fn spawn_hold_timer(sink: EventSink) -> Sender<TimerMsg> {
    let (tx, rx) = mpsc::channel::<TimerMsg>();
    std::thread::spawn(move || loop {
        let Ok(mut msg) = rx.recv() else { return };
        loop {
            let TimerMsg::Arm(ms) = msg else { break };
            match rx.recv_timeout(Duration::from_millis(ms as u64)) {
                Ok(newer) => msg = newer,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let cfg = CONFIG.snapshot();
                    let fg = foreground::current();
                    let excluded =
                        state::is_excluded(fg.as_ref().and_then(|f| f.exe_name.as_deref()));
                    let d = ACTIVATION.lock().unwrap().on_hold_timer(&cfg, excluded);
                    if let Some(ev) = d.event {
                        emit_semantic(&sink, ev);
                    }
                    break;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    tx
}

fn emit_semantic(sink: &EventSink, ev: Event) {
    let msg = match ev {
        Event::HoldActivated => AgentEvent::HoldActivated { foreground: foreground::current() },
        Event::HoldReleased => AgentEvent::HoldReleased,
        Event::ChordActivated => AgentEvent::ChordActivated { foreground: foreground::current() },
        Event::DismissedEsc => AgentEvent::Dismissed { reason: DismissReason::Esc },
        Event::DismissedChordAgain => AgentEvent::Dismissed { reason: DismissReason::ChordAgain },
        Event::DismissedPassthrough => {
            AgentEvent::Dismissed { reason: DismissReason::Passthrough }
        }
    };
    sink.send(msg);
}

/// stdin command pump (its own thread). Config lands in atomics only; the
/// tap callback never sees this thread.
fn command_pump(rx: Receiver<AgentCommand>, sink: EventSink) {
    use std::sync::atomic::Ordering::Relaxed;
    for cmd in rx {
        match cmd {
            AgentCommand::Configure {
                hold_enabled,
                hold_ms,
                chord_enabled,
                chord,
                excluded_exes,
                ..
            } => {
                CONFIG.hold_enabled.store(hold_enabled, Relaxed);
                CONFIG.hold_ms.store(hold_ms.clamp(150, 5000), Relaxed);
                CONFIG.chord_enabled.store(chord_enabled, Relaxed);
                CONFIG.chord_win.store(chord.win, Relaxed);
                CONFIG.chord_ctrl.store(chord.ctrl, Relaxed);
                CONFIG.chord_shift.store(chord.shift, Relaxed);
                CONFIG.chord_alt.store(chord.alt, Relaxed);
                CONFIG.chord_vk.store(chord.vk, Relaxed);
                if let Ok(mut list) = EXCLUDED.lock() {
                    *list = excluded_exes.iter().map(|e| state::normalize_identity(e)).collect();
                }
            }
            AgentCommand::SetOverlayVisible { visible } => {
                CONFIG.overlay_visible.store(visible, Relaxed);
            }
            // No taskbar on macOS: nothing to answer, by design.
            AgentCommand::QueryTaskbar => {}
            AgentCommand::SubscribeForeground { .. } => {}
            AgentCommand::Ping => sink.send(AgentEvent::Pong),
            AgentCommand::Shutdown => std::process::exit(0),
            _ => {}
        }
    }
    // stdin closed => the app exited.
    std::process::exit(0);
}

fn on_tap_event(
    sink: &EventSink,
    timer_tx: &Sender<TimerMsg>,
    etype: CGEventType,
    event: &CGEvent,
) -> CallbackResult {
    match etype {
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
            eprintln!("tap disabled ({etype:?}), re-enabling");
            reenable_tap();
            return CallbackResult::Keep;
        }
        CGEventType::KeyDown | CGEventType::KeyUp | CGEventType::FlagsChanged => {}
        _ => return CallbackResult::Keep,
    }
    let Some((vk, down)) = translate(etype, event) else {
        return CallbackResult::Keep;
    };
    let cfg = CONFIG.snapshot();
    let d = ACTIVATION.lock().unwrap().on_key(&cfg, vk, down);
    if let Some(cmd) = d.timer {
        let _ = timer_tx.send(match cmd {
            TimerCmd::Arm(ms) => TimerMsg::Arm(ms),
            TimerCmd::Cancel => TimerMsg::Cancel,
        });
    }
    if let Some(ev) = d.event {
        emit_semantic(sink, ev);
    }
    if d.swallow {
        CallbackResult::Drop
    } else {
        CallbackResult::Keep
    }
}

/// One key transition as (Windows-style VK, is_down), or None to ignore.
fn translate(etype: CGEventType, event: &CGEvent) -> Option<(u32, bool)> {
    let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
    match etype {
        CGEventType::KeyDown => Some((vk_for_keycode(keycode)?, true)),
        CGEventType::KeyUp => Some((vk_for_keycode(keycode)?, false)),
        CGEventType::FlagsChanged => {
            // The event carries the modifier's keycode; whether it went down
            // or up is in the *device-dependent* flag bits (the generic
            // CGEventFlagCommand bit stays set while either ⌘ is held).
            let down = event.get_flags().bits() & device_bit(keycode)? != 0;
            Some((vk_for_keycode(keycode)?, down))
        }
        _ => None,
    }
}

/// NX_DEVICE* masks from IOLLEvent.h for the per-side modifier keys.
fn device_bit(keycode: u16) -> Option<u64> {
    Some(match keycode {
        KeyCode::CONTROL => 0x0000_0001,
        KeyCode::SHIFT => 0x0000_0002,
        KeyCode::RIGHT_SHIFT => 0x0000_0004,
        KeyCode::COMMAND => 0x0000_0008,
        KeyCode::RIGHT_COMMAND => 0x0000_0010,
        KeyCode::OPTION => 0x0000_0020,
        KeyCode::RIGHT_OPTION => 0x0000_0040,
        KeyCode::RIGHT_CONTROL => 0x0000_2000,
        _ => return None,
    })
}

/// CGKeyCode (ANSI layout positions) → Windows virtual-key code, covering
/// every key the settings UI can capture into a `ChordSpec` plus the
/// modifiers/Esc the state machine tracks. Unknown keys stay None and pass
/// through untouched.
fn vk_for_keycode(keycode: u16) -> Option<u32> {
    Some(match keycode {
        // Modifiers (⌘ maps to the Win VKs; see activation.rs).
        KeyCode::COMMAND => 0x5B,
        KeyCode::RIGHT_COMMAND => 0x5C,
        KeyCode::SHIFT => 0xA0,
        KeyCode::RIGHT_SHIFT => 0xA1,
        KeyCode::CONTROL => 0xA2,
        KeyCode::RIGHT_CONTROL => 0xA3,
        KeyCode::OPTION => 0xA4,
        KeyCode::RIGHT_OPTION => 0xA5,
        // Controls & navigation.
        KeyCode::ESCAPE => 0x1B,
        KeyCode::RETURN => 0x0D,
        KeyCode::TAB => 0x09,
        KeyCode::SPACE => 0x20,
        KeyCode::DELETE => 0x08, // backspace
        KeyCode::FORWARD_DELETE => 0x2E,
        KeyCode::HOME => 0x24,
        KeyCode::END => 0x23,
        KeyCode::PAGE_UP => 0x21,
        KeyCode::PAGE_DOWN => 0x22,
        KeyCode::LEFT_ARROW => 0x25,
        KeyCode::UP_ARROW => 0x26,
        KeyCode::RIGHT_ARROW => 0x27,
        KeyCode::DOWN_ARROW => 0x28,
        // Letters.
        KeyCode::ANSI_A => 0x41,
        KeyCode::ANSI_B => 0x42,
        KeyCode::ANSI_C => 0x43,
        KeyCode::ANSI_D => 0x44,
        KeyCode::ANSI_E => 0x45,
        KeyCode::ANSI_F => 0x46,
        KeyCode::ANSI_G => 0x47,
        KeyCode::ANSI_H => 0x48,
        KeyCode::ANSI_I => 0x49,
        KeyCode::ANSI_J => 0x4A,
        KeyCode::ANSI_K => 0x4B,
        KeyCode::ANSI_L => 0x4C,
        KeyCode::ANSI_M => 0x4D,
        KeyCode::ANSI_N => 0x4E,
        KeyCode::ANSI_O => 0x4F,
        KeyCode::ANSI_P => 0x50,
        KeyCode::ANSI_Q => 0x51,
        KeyCode::ANSI_R => 0x52,
        KeyCode::ANSI_S => 0x53,
        KeyCode::ANSI_T => 0x54,
        KeyCode::ANSI_U => 0x55,
        KeyCode::ANSI_V => 0x56,
        KeyCode::ANSI_W => 0x57,
        KeyCode::ANSI_X => 0x58,
        KeyCode::ANSI_Y => 0x59,
        KeyCode::ANSI_Z => 0x5A,
        // Digit row.
        KeyCode::ANSI_0 => 0x30,
        KeyCode::ANSI_1 => 0x31,
        KeyCode::ANSI_2 => 0x32,
        KeyCode::ANSI_3 => 0x33,
        KeyCode::ANSI_4 => 0x34,
        KeyCode::ANSI_5 => 0x35,
        KeyCode::ANSI_6 => 0x36,
        KeyCode::ANSI_7 => 0x37,
        KeyCode::ANSI_8 => 0x38,
        KeyCode::ANSI_9 => 0x39,
        // Function keys.
        KeyCode::F1 => 0x70,
        KeyCode::F2 => 0x71,
        KeyCode::F3 => 0x72,
        KeyCode::F4 => 0x73,
        KeyCode::F5 => 0x74,
        KeyCode::F6 => 0x75,
        KeyCode::F7 => 0x76,
        KeyCode::F8 => 0x77,
        KeyCode::F9 => 0x78,
        KeyCode::F10 => 0x79,
        KeyCode::F11 => 0x7A,
        KeyCode::F12 => 0x7B,
        // OEM punctuation (VK_OEM_*).
        KeyCode::ANSI_SEMICOLON => 0xBA,
        KeyCode::ANSI_EQUAL => 0xBB,
        KeyCode::ANSI_COMMA => 0xBC,
        KeyCode::ANSI_MINUS => 0xBD,
        KeyCode::ANSI_PERIOD => 0xBE,
        KeyCode::ANSI_SLASH => 0xBF,
        KeyCode::ANSI_GRAVE => 0xC0,
        KeyCode::ANSI_LEFT_BRACKET => 0xDB,
        KeyCode::ANSI_BACKSLASH => 0xDC,
        KeyCode::ANSI_RIGHT_BRACKET => 0xDD,
        KeyCode::ANSI_QUOTE => 0xDE,
        _ => return None,
    })
}

/// Install the tap and run the main-thread run loop until stdin closes /
/// Shutdown (both exit the process from the pump thread).
pub fn run() -> anyhow::Result<()> {
    let sink = ipc::spawn_writer();
    let commands = ipc::spawn_reader();

    if !unsafe { AXIsProcessTrusted() } {
        sink.send(AgentEvent::Fatal {
            kind: FatalKind::PermissionRequired,
            message: "Accessibility permission missing: System Settings → Privacy & Security → \
                      Accessibility → enable Quicuts (or the terminal that launched it, in dev)"
                .into(),
        });
        // Let the writer thread flush the line before exiting.
        std::thread::sleep(Duration::from_millis(100));
        anyhow::bail!("accessibility permission missing");
    }

    {
        let sink = sink.clone();
        std::thread::spawn(move || command_pump(commands, sink));
    }
    let timer_tx = spawn_hold_timer(sink.clone());
    foreground::install(sink.clone());

    let tap_sink = sink.clone();
    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![CGEventType::KeyDown, CGEventType::KeyUp, CGEventType::FlagsChanged],
        move |_proxy: CGEventTapProxy, etype, event| {
            on_tap_event(&tap_sink, &timer_tx, etype, event)
        },
    );
    let tap = match tap {
        Ok(t) => t,
        Err(()) => {
            sink.send(AgentEvent::Fatal {
                kind: FatalKind::HookInstallFailed,
                message: "CGEventTapCreate returned NULL (permission revoked mid-flight?)".into(),
            });
            std::thread::sleep(Duration::from_millis(100));
            anyhow::bail!("event tap creation failed");
        }
    };
    let source = tap
        .mach_port()
        .create_runloop_source(0)
        .map_err(|_| anyhow::anyhow!("tap run-loop source creation failed"))?;
    CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();
    TAP_PORT.store(
        tap.mach_port().as_concrete_TypeRef() as *mut _,
        Ordering::Release,
    );

    // Watchdog: a tap killed while no events flow (sleep/wake, lock/unlock)
    // never gets a TapDisabled callback; poll and revive.
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(5));
        let port = TAP_PORT.load(Ordering::Acquire);
        if !port.is_null() && !unsafe { CGEventTapIsEnabled(port as _) } {
            eprintln!("tap found disabled by watchdog, re-enabling");
            reenable_tap();
        }
    });

    sink.send(AgentEvent::Ready {
        proto_version: PROTO_VERSION,
        caps: vec![caps::HOLD.into(), caps::CHORD.into(), caps::FOREGROUND.into()],
    });

    CFRunLoop::run_current();
    Ok(())
}
