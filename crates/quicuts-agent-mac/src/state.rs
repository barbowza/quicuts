//! Hot state shared between the event-tap callback and the rest of the
//! agent. The tap callback must never block on IPC, so everything it reads
//! is a plain atomic (or a mutex only the stdin thread writes) updated
//! elsewhere.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use crate::activation;

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
    /// Set by the app via SetOverlayVisible; gates Esc/chord-again handling.
    pub overlay_visible: AtomicBool,
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
        }
    }

    /// Materialize the snapshot the pure state machine consumes.
    pub fn snapshot(&self) -> activation::Config {
        let o = Ordering::Relaxed;
        activation::Config {
            hold_enabled: self.hold_enabled.load(o),
            hold_ms: self.hold_ms.load(o),
            chord_enabled: self.chord_enabled.load(o),
            chord_win: self.chord_win.load(o),
            chord_ctrl: self.chord_ctrl.load(o),
            chord_shift: self.chord_shift.load(o),
            chord_alt: self.chord_alt.load(o),
            chord_vk: self.chord_vk.load(o),
            overlay_visible: self.overlay_visible.load(o),
        }
    }
}

/// Global config instance, read from the tap callback.
pub static CONFIG: Config = Config::new();

/// Lowercase identities (bundle ids here) for which hold must not arm.
pub static EXCLUDED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Normalize an identity for exclusion matching, mirroring
/// `match_foreground`'s rule (lowercase, strip one trailing ".exe") so a
/// Windows-shaped settings list still behaves; on macOS these are bundle ids.
pub fn normalize_identity(name: &str) -> String {
    let base = name.rsplit(['\\', '/']).next().unwrap_or(name).to_ascii_lowercase();
    base.strip_suffix(".exe").unwrap_or(&base).to_string()
}

pub fn is_excluded(identity: Option<&str>) -> bool {
    let Some(name) = identity else {
        return false;
    };
    let norm = normalize_identity(name);
    EXCLUDED
        .lock()
        .map(|list| list.iter().any(|e| *e == norm))
        .unwrap_or(false)
}
