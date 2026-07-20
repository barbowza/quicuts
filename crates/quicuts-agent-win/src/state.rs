//! Hot state shared between the low-level hook callback and the rest of the
//! agent. The hook callback must be allocation-free and must never block on
//! IPC, so everything it reads is a plain atomic updated elsewhere.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// dwExtraInfo marker on every event we inject, so the hook ignores its own
/// synthetic keystrokes.
pub const QUICUTS_MAGIC: usize = 0x5175_4943; // "QuIC"

/// Dummy VK we inject to absorb a lone Win-up (prevents the Start menu).
/// 0xFF is VK__none_ / unassigned.
pub const VK_DUMMY: u16 = 0xFF;

#[derive(Debug, Default)]
pub struct Config {
    pub hold_enabled: AtomicBool,
    pub hold_ms: AtomicU32,
    pub chord_enabled: AtomicBool,
    pub chord_win: AtomicBool,
    pub chord_ctrl: AtomicBool,
    pub chord_shift: AtomicBool,
    pub chord_alt: AtomicBool,
    pub chord_vk: AtomicU32,
    /// Set by the app via SetOverlayVisible; gates Esc/passthrough handling.
    pub overlay_visible: AtomicBool,
    /// Experimental title detection: watch foreground title changes and
    /// re-emit ForegroundChanged (debounced). Gates the NAMECHANGE hook.
    pub title_events: AtomicBool,
}

impl Config {
    pub const fn new() -> Self {
        Self {
            hold_enabled: AtomicBool::new(false),
            hold_ms: AtomicU32::new(900),
            chord_enabled: AtomicBool::new(false),
            chord_win: AtomicBool::new(false),
            chord_ctrl: AtomicBool::new(false),
            chord_shift: AtomicBool::new(false),
            chord_alt: AtomicBool::new(false),
            chord_vk: AtomicU32::new(0xBF),
            overlay_visible: AtomicBool::new(false),
            title_events: AtomicBool::new(false),
        }
    }

    pub fn overlay_visible(&self) -> bool {
        self.overlay_visible.load(Ordering::Relaxed)
    }
}

/// Global config instance, read from the hook callback.
pub static CONFIG: Config = Config::new();
