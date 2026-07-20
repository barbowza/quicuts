//! Low-level keyboard hook, activation state machine, and message pump.
//!
//! The `WH_KEYBOARD_LL` callback and the `WM_TIMER` handler both run on the
//! pump thread, so hook state lives in a thread-local touched only there. The
//! stdin thread mutates `CONFIG` atomics and `EXCLUDED` only. The callback is
//! allocation-free on the hot path and never blocks on IPC.

use std::cell::RefCell;
use std::sync::mpsc::TryRecvError;
use std::sync::{Mutex, OnceLock};

use quicuts_proto::{caps, AgentCommand, AgentEvent, DismissReason, PROTO_VERSION};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::Accessibility::{
    SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, KillTimer, PeekMessageW, SetTimer,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, EVENT_OBJECT_NAMECHANGE,
    EVENT_SYSTEM_FOREGROUND, HHOOK, KBDLLHOOKSTRUCT, MSG, OBJID_WINDOW, PM_REMOVE,
    WH_KEYBOARD_LL, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER,
};

use crate::foreground;
use crate::ipc::{self, EventSink};
use crate::state::{CONFIG, QUICUTS_MAGIC, VK_DUMMY};

const VK_LWIN: u32 = 0x5B;
const VK_RWIN: u32 = 0x5C;
const VK_ESCAPE: u32 = 0x1B;

static SINK: OnceLock<EventSink> = OnceLock::new();
static HOOK: OnceLock<isize> = OnceLock::new();
/// Lowercase, ".exe"-stripped exe names for which hold must not arm.
static EXCLUDED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Whether the tray overflow flyout currently holds the foreground.
static FLYOUT_OPEN: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    WinDown,
    HoldActive,
    Combo,
}

struct HookState {
    phase: Phase,
    win_down: bool,
    shift: bool,
    ctrl: bool,
    alt: bool,
    /// A key-up we must swallow (chord key, Esc).
    swallow_up: Option<u32>,
    /// The OS-assigned timer id (SetTimer with a NULL window generates its own
    /// id and ignores ours), 0 when disarmed.
    timer_id: usize,
}

thread_local! {
    static STATE: RefCell<HookState> = const { RefCell::new(HookState {
        phase: Phase::Idle,
        win_down: false,
        shift: false,
        ctrl: false,
        alt: false,
        swallow_up: None,
        timer_id: 0,
    }) };
}

/// Debounce before re-reporting a foreground title change: sites like Gmail
/// mutate the title several times in a burst (unread counts, loading).
const TITLE_DEBOUNCE_MS: u32 = 200;

/// Experimental title watcher (ADR 0003): a second WinEvent hook, installed
/// only while `Configure.title_events_enabled` is on, that re-emits
/// `ForegroundChanged` when the foreground window's title settles. Pump
/// thread only, like `HookState`.
struct TitleWatch {
    /// HWINEVENTHOOK.0; null when not installed.
    hook: isize,
    /// Debounce timer id (SetTimer with a NULL window), 0 when disarmed.
    timer_id: usize,
    /// Window whose title changed since the last flush.
    pending_hwnd: isize,
    /// Last (hwnd, title) actually emitted, to drop no-op re-reads.
    last_emitted: Option<(isize, String)>,
}

thread_local! {
    static TITLE_WATCH: RefCell<TitleWatch> = const { RefCell::new(TitleWatch {
        hook: 0,
        timer_id: 0,
        pending_hwnd: 0,
        last_emitted: None,
    }) };
}

fn emit(ev: AgentEvent) {
    if let Some(sink) = SINK.get() {
        sink.send(ev);
    }
}

fn is_win(vk: u32) -> bool {
    vk == VK_LWIN || vk == VK_RWIN
}

fn is_modifier(vk: u32) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12 | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4 | 0xA5
    ) || is_win(vk)
}

/// Update tracked modifier booleans; returns nothing.
fn track_modifier(st: &mut HookState, vk: u32, down: bool) {
    match vk {
        0x10 | 0xA0 | 0xA1 => st.shift = down,
        0x11 | 0xA2 | 0xA3 => st.ctrl = down,
        0x12 | 0xA4 | 0xA5 => st.alt = down,
        v if is_win(v) => st.win_down = down,
        _ => {}
    }
}

fn chord_matches(st: &HookState, vk: u32) -> bool {
    use std::sync::atomic::Ordering::Relaxed;
    if !CONFIG.chord_enabled.load(Relaxed) {
        return false;
    }
    if vk != CONFIG.chord_vk.load(Relaxed) {
        return false;
    }
    st.win_down == CONFIG.chord_win.load(Relaxed)
        && st.shift == CONFIG.chord_shift.load(Relaxed)
        && st.ctrl == CONFIG.chord_ctrl.load(Relaxed)
        && st.alt == CONFIG.chord_alt.load(Relaxed)
}

/// Inject a dummy key (down+up, marked with our magic) so the shell treats
/// the preceding lone-Win press as a combo and does not open Start.
fn inject_dummy() {
    let mk = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(VK_DUMMY),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: QUICUTS_MAGIC,
            },
        },
    };
    let inputs = [mk(KEYBD_EVENT_FLAGS(0)), mk(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// WinEvent callback: report foreground changes (skipping our own windows) so
/// the app can pre-assemble the page for the new app.
unsafe extern "system" fn winevent_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if event != EVENT_SYSTEM_FOREGROUND || id_object != OBJID_WINDOW.0 || id_child != 0 {
        return;
    }
    // Alt+Tab staging windows are topmost + shell-owned, so they must be
    // classified before the flyout heuristic; they mean neither a new
    // foreground app nor a flyout transition.
    if foreground::is_transient_shell(hwnd) {
        return;
    }
    // The tray overflow flyout is a semantic transition of its own: the app
    // ducks the overlay under it. It must not be reported as a foreground
    // app (that would flip the page to "File Explorer"), and any *other*
    // window taking the foreground means the flyout closed.
    use std::sync::atomic::Ordering::Relaxed;
    if foreground::is_tray_flyout(hwnd) {
        if !FLYOUT_OPEN.swap(true, Relaxed) {
            eprintln!("shell flyout open");
            emit(AgentEvent::ShellFlyoutChanged {
                open: true,
                window: Some(hwnd.0 as usize as u64),
            });
        }
        return;
    }
    if FLYOUT_OPEN.swap(false, Relaxed) {
        eprintln!("shell flyout closed");
        emit(AgentEvent::ShellFlyoutChanged { open: false, window: None });
    }
    if let Some(info) = foreground::report_for_hwnd(hwnd) {
        // A window switch carries a fresh title already; drop any pending
        // debounce so the title watcher can't emit a stale window's title.
        reset_title_watch();
        emit(AgentEvent::ForegroundChanged { foreground: info });
    }
}

/// NAMECHANGE callback for the experimental title watcher: note that the
/// foreground window's title is dirty and (re)arm the debounce timer.
/// Runs on the pump thread (WINEVENT_OUTOFCONTEXT hooks dispatch during
/// message processing on the registering thread).
unsafe extern "system" fn title_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    use std::sync::atomic::Ordering::Relaxed;
    if event != EVENT_OBJECT_NAMECHANGE || id_object != OBJID_WINDOW.0 || id_child != 0 {
        return;
    }
    // Belt and braces: the hook is uninstalled when disabled, but a queued
    // event may still land right after a Configure.
    if !CONFIG.title_events.load(Relaxed) {
        return;
    }
    // Only the foreground window's title matters (tab switches, SPA
    // navigation); background renames are noise.
    if hwnd != GetForegroundWindow() {
        return;
    }
    if foreground::is_transient_shell(hwnd) || foreground::is_tray_flyout(hwnd) {
        return;
    }
    TITLE_WATCH.with(|t| {
        let mut tw = t.borrow_mut();
        tw.pending_hwnd = hwnd.0 as isize;
        if tw.timer_id != 0 {
            let _ = KillTimer(None, tw.timer_id);
        }
        tw.timer_id = SetTimer(None, 0, TITLE_DEBOUNCE_MS, None);
    });
}

/// The title debounce elapsed: if the dirty window still holds the
/// foreground and its title actually changed, re-emit ForegroundChanged.
fn flush_title() {
    TITLE_WATCH.with(|t| {
        let mut tw = t.borrow_mut();
        if tw.timer_id != 0 {
            unsafe {
                let _ = KillTimer(None, tw.timer_id);
            }
            tw.timer_id = 0;
        }
        let hwnd = HWND(tw.pending_hwnd as *mut _);
        tw.pending_hwnd = 0;
        if hwnd.is_invalid() || hwnd != unsafe { GetForegroundWindow() } {
            return;
        }
        let Some(info) = foreground::report_for_hwnd(hwnd) else {
            return;
        };
        let key = (hwnd.0 as isize, info.title.clone().unwrap_or_default());
        if tw.last_emitted.as_ref() == Some(&key) {
            return;
        }
        tw.last_emitted = Some(key);
        emit(AgentEvent::ForegroundChanged { foreground: info });
    });
}

/// Cancel any pending debounce and forget the dedup state (the hook itself
/// stays installed). Called on real foreground switches.
fn reset_title_watch() {
    TITLE_WATCH.with(|t| {
        let mut tw = t.borrow_mut();
        if tw.timer_id != 0 {
            unsafe {
                let _ = KillTimer(None, tw.timer_id);
            }
            tw.timer_id = 0;
        }
        tw.pending_hwnd = 0;
        tw.last_emitted = None;
    });
}

/// Install or remove the NAMECHANGE hook to match the Configure flag. Must
/// run on the pump thread (it owns the message loop the hook dispatches
/// on); `apply_command` does.
fn set_title_watch(enabled: bool) {
    TITLE_WATCH.with(|t| {
        let installed = t.borrow().hook != 0;
        if enabled && !installed {
            let hook = unsafe {
                SetWinEventHook(
                    EVENT_OBJECT_NAMECHANGE,
                    EVENT_OBJECT_NAMECHANGE,
                    None,
                    Some(title_event_proc),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            };
            if hook.0.is_null() {
                eprintln!("title watch: SetWinEventHook(NAMECHANGE) failed");
            } else {
                eprintln!("title watch: installed");
                t.borrow_mut().hook = hook.0 as isize;
            }
        } else if !enabled && installed {
            let hook = std::mem::take(&mut t.borrow_mut().hook);
            unsafe {
                let _ = UnhookWinEvent(HWINEVENTHOOK(hook as *mut _));
            }
            eprintln!("title watch: removed");
        }
    });
    if !enabled {
        reset_title_watch();
    }
}

/// The hook callback. Returns 1 to swallow, else forwards.
unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    // Ignore our own injected events.
    if kb.dwExtraInfo == QUICUTS_MAGIC {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let vk = kb.vkCode;
    let msg = wparam.0 as u32;
    let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

    let swallow = STATE.with(|s| {
        let mut st = s.borrow_mut();
        track_modifier(&mut st, vk, is_down);
        decide(&mut st, vk, is_down, is_up)
    });

    if swallow {
        LRESULT(1)
    } else {
        CallNextHookEx(None, code, wparam, lparam)
    }
}

/// Core decision. Returns true to swallow the event.
fn decide(st: &mut HookState, vk: u32, is_down: bool, is_up: bool) -> bool {
    // Swallow the matching key-up of a previously swallowed key (chord, Esc).
    if is_up {
        if let Some(target) = st.swallow_up {
            if target == vk {
                st.swallow_up = None;
                return true;
            }
        }
    }

    // --- Chord (checked before phase logic) ---
    if is_down && !is_modifier(vk) && chord_matches(st, vk) {
        // Autorepeat of a still-held chord key: swallow without re-emitting,
        // otherwise one press toggles the overlay several times.
        if st.swallow_up == Some(vk) {
            return true;
        }
        st.swallow_up = Some(vk);
        if CONFIG.chord_win.load(std::sync::atomic::Ordering::Relaxed) {
            inject_dummy();
        }
        // Always report activation; the app decides show/focus/hide since
        // only it knows whether the overlay is visible and focused.
        emit(AgentEvent::ChordActivated {
            foreground: foreground::current_foreground(),
        });
        return true;
    }

    // --- Esc dismisses while the overlay is up (swallow down and its up) ---
    if is_down && vk == VK_ESCAPE && CONFIG.overlay_visible() {
        st.swallow_up = Some(VK_ESCAPE);
        emit(AgentEvent::Dismissed { reason: DismissReason::Esc });
        return true;
    }

    match st.phase {
        Phase::Idle => {
            if is_down && is_win(vk) {
                st.phase = Phase::WinDown;
                if CONFIG.hold_enabled.load(std::sync::atomic::Ordering::Relaxed) {
                    arm_timer(st);
                }
            }
            false
        }
        Phase::WinDown => {
            if is_up && is_win(vk) {
                // Quick Win tap: let it through (Start opens normally).
                disarm_timer(st);
                st.phase = Phase::Idle;
                false
            } else if is_down && !is_win(vk) {
                // Any other key -> real combo, cancel hold.
                disarm_timer(st);
                st.phase = Phase::Combo;
                false
            } else {
                false
            }
        }
        Phase::HoldActive => {
            if is_up && is_win(vk) {
                // Absorb the lone Win-up so Start does not open, then release.
                inject_dummy();
                st.phase = Phase::Idle;
                emit(AgentEvent::HoldReleased);
                false
            } else if is_down && !is_modifier(vk) {
                // Win+E etc.: pass through, dismiss, become a combo. A lone
                // modifier does NOT dismiss: it triggers no OS shortcut until
                // a real key follows, and tao's set_focus injects a synthetic
                // Alt tap (dwExtraInfo=0) when SetForegroundWindow is denied —
                // on the first activation after launch — which would otherwise
                // dismiss the panel the instant it opens.
                st.phase = Phase::Combo;
                emit(AgentEvent::Dismissed { reason: DismissReason::Passthrough });
                false
            } else {
                false
            }
        }
        Phase::Combo => {
            if is_up && is_win(vk) {
                st.phase = Phase::Idle;
            }
            false
        }
    }
}

fn arm_timer(st: &mut HookState) {
    use std::sync::atomic::Ordering::Relaxed;
    disarm_timer(st);
    let ms = CONFIG.hold_ms.load(Relaxed);
    // With a NULL window SetTimer returns a freshly generated id; keep it so
    // we can kill exactly this timer later.
    let id = unsafe { SetTimer(None, 0, ms, None) };
    st.timer_id = id;
}

fn disarm_timer(st: &mut HookState) {
    if st.timer_id != 0 {
        unsafe {
            let _ = KillTimer(None, st.timer_id);
        }
        st.timer_id = 0;
    }
}

/// A modifier held alongside Win means a combo (Ctrl+Win+arrow, Shift+Win+S
/// pressed other-key-first), not a hold. Pressed *before* Win-down it never
/// produces another key event while we sit in WinDown, so only the tracked
/// state can see it — but tracked state alone can go stale (key-ups typed
/// into elevated windows never reach an unelevated hook), so require the OS
/// to confirm the key is still physically down. Never sweep the whole
/// keyboard here: some drivers report phantom VKs as permanently down,
/// which would suppress every hold.
fn combo_modifier_down(st: &HookState) -> Option<&'static str> {
    let checks: [(bool, i32, &'static str); 3] =
        [(st.shift, 0x10, "shift"), (st.ctrl, 0x11, "ctrl"), (st.alt, 0x12, "alt")];
    checks.into_iter().find_map(|(claimed, vk, name)| {
        (claimed && (unsafe { GetAsyncKeyState(vk) } as u16) & 0x8000 != 0).then_some(name)
    })
}

/// WM_TIMER: the hold threshold elapsed. Promote to HoldActive unless the
/// foreground app is excluded or the Win press is part of a combo.
fn on_timer() {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        disarm_timer(&mut st);
        if st.phase != Phase::WinDown {
            return;
        }
        if let Some(m) = combo_modifier_down(&st) {
            // A combo is in progress; leave it entirely to the OS.
            eprintln!("hold suppressed: {m} held with win (combo)");
            st.phase = Phase::Combo;
            return;
        }
        let fg = foreground::current_foreground();
        if is_excluded(fg.as_ref()) {
            // Leave the Win press alone; user can still combo/tap.
            return;
        }
        st.phase = Phase::HoldActive;
        emit(AgentEvent::HoldActivated { foreground: fg });
    });
}

fn is_excluded(fg: Option<&quicuts_proto::ForegroundInfo>) -> bool {
    let Some(name) = fg.and_then(|f| f.exe_name.as_deref()) else {
        return false;
    };
    let norm = quicuts_manifest_normalize(name);
    EXCLUDED
        .lock()
        .map(|list| list.iter().any(|e| *e == norm))
        .unwrap_or(false)
}

/// Minimal exe normalization (lowercase, strip trailing ".exe"); kept local so
/// the agent doesn't depend on the manifest crate.
fn quicuts_manifest_normalize(name: &str) -> String {
    let base = name.rsplit(['\\', '/']).next().unwrap_or(name).to_ascii_lowercase();
    base.strip_suffix(".exe").unwrap_or(&base).to_string()
}

fn apply_command(cmd: AgentCommand) -> bool {
    use std::sync::atomic::Ordering::Relaxed;
    match cmd {
        AgentCommand::Configure {
            hold_enabled,
            hold_ms,
            chord_enabled,
            chord,
            excluded_exes,
            title_events_enabled,
        } => {
            CONFIG.hold_enabled.store(hold_enabled, Relaxed);
            CONFIG.hold_ms.store(hold_ms.clamp(150, 5000), Relaxed);
            CONFIG.chord_enabled.store(chord_enabled, Relaxed);
            CONFIG.chord_win.store(chord.win, Relaxed);
            CONFIG.chord_ctrl.store(chord.ctrl, Relaxed);
            CONFIG.chord_shift.store(chord.shift, Relaxed);
            CONFIG.chord_alt.store(chord.alt, Relaxed);
            CONFIG.chord_vk.store(chord.vk, Relaxed);
            CONFIG.title_events.store(title_events_enabled, Relaxed);
            set_title_watch(title_events_enabled);
            if let Ok(mut list) = EXCLUDED.lock() {
                *list = excluded_exes
                    .iter()
                    .map(|e| quicuts_manifest_normalize(e))
                    .collect();
            }
        }
        AgentCommand::SetOverlayVisible { visible } => {
            CONFIG.overlay_visible.store(visible, Relaxed);
        }
        AgentCommand::QueryTaskbar => {
            // UIA reads can take hundreds of ms; keep them off this thread —
            // it owns the keyboard hook, and a stall here delays all input.
            std::thread::spawn(|| match crate::taskbar::read_snapshot() {
                Some(snap) => {
                    let heads: Vec<String> = snap
                        .buttons
                        .iter()
                        .take(3)
                        .map(|b| format!("#{}@{},{} {}x{}", b.index, b.rect.x, b.rect.y, b.rect.w, b.rect.h))
                        .collect();
                    eprintln!(
                        "taskbar: {} buttons, rect {:?}, edge {:?}, first: {}",
                        snap.buttons.len(),
                        snap.taskbar_rect,
                        snap.edge,
                        heads.join(" ")
                    );
                    emit(AgentEvent::Taskbar(snap));
                }
                None => eprintln!("taskbar: ABM_GETTASKBARPOS failed, no snapshot"),
            });
        }
        AgentCommand::SubscribeForeground { .. } => {}
        AgentCommand::Ping => emit(AgentEvent::Pong),
        AgentCommand::Shutdown => return false,
        _ => {}
    }
    true
}

/// Install the hook and run the message pump until stdin closes / shutdown.
pub fn run() -> anyhow::Result<()> {
    let sink = ipc::spawn_writer();
    let _ = SINK.set(sink);
    let commands = ipc::spawn_reader();

    let hook = unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0)
    };
    let hook = match hook {
        Ok(h) => h,
        Err(e) => {
            emit(AgentEvent::Fatal {
                kind: quicuts_proto::FatalKind::HookInstallFailed,
                message: format!("SetWindowsHookExW failed: {e}"),
            });
            anyhow::bail!("hook install failed: {e}");
        }
    };
    let _ = HOOK.set(hook.0 as isize);

    // Foreground watcher (best-effort; failure just means no pre-assembly).
    let winevent = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(winevent_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };

    emit(AgentEvent::Ready {
        proto_version: PROTO_VERSION,
        caps: vec![
            caps::HOLD.into(),
            caps::CHORD.into(),
            caps::FOREGROUND.into(),
            caps::TASKBAR.into(),
            caps::TITLE.into(),
        ],
    });

    // Cooperative pump: PeekMessage keeps us responsive to stdin without a
    // separate wake mechanism; a short sleep bounds idle CPU.
    let mut running = true;
    while running {
        // Drain pending commands.
        loop {
            match commands.try_recv() {
                Ok(cmd) => {
                    if !apply_command(cmd) {
                        running = false;
                        break;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    running = false;
                    break;
                }
            }
        }
        if !running {
            break;
        }
        // Dispatch Windows messages (WM_TIMER, hook maintenance).
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, Some(HWND::default()), 0, 0, PM_REMOVE).as_bool() {
                // Two thread timers can run here (hold + title debounce);
                // WM_TIMER's wParam carries the SetTimer-generated id, so
                // dispatch by id — treating every WM_TIMER as the hold
                // timer would fire phantom hold activations.
                if msg.message == WM_TIMER {
                    let id = msg.wParam.0;
                    let is_hold = STATE.with(|s| s.borrow().timer_id == id);
                    if is_hold {
                        on_timer();
                    } else if TITLE_WATCH.with(|t| t.borrow().timer_id == id) {
                        flush_title();
                    }
                } else {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    set_title_watch(false);
    unsafe {
        let _ = UnhookWindowsHookEx(HHOOK(hook.0));
        if !winevent.0.is_null() {
            let _ = UnhookWinEvent(winevent);
        }
    }
    Ok(())
}
