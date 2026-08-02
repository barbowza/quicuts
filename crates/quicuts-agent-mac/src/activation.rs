//! Pure hold/chord activation state machine — no AppKit, no CoreGraphics,
//! no I/O, no clocks. The event tap feeds it key transitions as plain data
//! and acts on the returned [`Decision`]; the hold timer is owned by the
//! caller and driven via [`TimerCmd`].
//!
//! Ported from the Windows agent's `hook.rs` (`Idle → CmdDown → {Combo |
//! HoldActive} → Idle`), minus the Start-menu dummy-key injection, which has
//! no macOS equivalent. Keys are identified by the Windows-style virtual-key
//! codes the protocol speaks; the tap adapter translates CGKeyCode → VK
//! before calling in. ⌘ arrives as VK_LWIN/VK_RWIN.

pub const VK_LWIN: u32 = 0x5B;
pub const VK_RWIN: u32 = 0x5C;
pub const VK_ESCAPE: u32 = 0x1B;

/// Snapshot of the agent configuration relevant to activation. The adapter
/// materializes this from its cached atomics on every callback.
#[derive(Debug, Clone, Copy, Default)]
pub struct Config {
    pub hold_enabled: bool,
    pub hold_ms: u32,
    pub chord_enabled: bool,
    pub chord_win: bool,
    pub chord_ctrl: bool,
    pub chord_shift: bool,
    pub chord_alt: bool,
    pub chord_vk: u32,
    /// Mirrors the app's `SetOverlayVisible`; gates Esc/chord-again handling.
    pub overlay_visible: bool,
}

/// Semantic transition to report to the app. The adapter attaches the
/// current foreground info where the protocol carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    HoldActivated,
    HoldReleased,
    ChordActivated,
    DismissedEsc,
    DismissedChordAgain,
    DismissedPassthrough,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerCmd {
    /// (Re)arm the hold timer for this many ms.
    Arm(u32),
    Cancel,
}

/// What the adapter must do with the event it just delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Decision {
    /// Swallow the CGEvent (return None from the tap callback).
    pub swallow: bool,
    pub event: Option<Event>,
    pub timer: Option<TimerCmd>,
}

impl Decision {
    const PASS: Decision = Decision { swallow: false, event: None, timer: None };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    /// ⌘ down, hold timer possibly armed, nothing else seen yet.
    CmdDown,
    /// The hold threshold elapsed with ⌘ still down; the panel is up.
    HoldActive,
    /// ⌘ participates in an OS shortcut (⌘Tab etc.); stay out of the way
    /// until it is released.
    Combo,
}

#[derive(Debug)]
pub struct Activation {
    phase: Phase,
    /// ⌘ is tracked per side: with both keys held, releasing one must not
    /// end the hold. `cmd_down()` is the "any ⌘ still down" predicate.
    lcmd: bool,
    rcmd: bool,
    shift: bool,
    ctrl: bool,
    alt: bool,
    /// A key-up we must swallow (chord key, Esc).
    swallow_up: Option<u32>,
}

impl Default for Activation {
    fn default() -> Self {
        Self::new()
    }
}

fn is_cmd(vk: u32) -> bool {
    vk == VK_LWIN || vk == VK_RWIN
}

fn is_modifier(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4 | 0xA5) || is_cmd(vk)
}

impl Activation {
    pub const fn new() -> Self {
        Self {
            phase: Phase::Idle,
            lcmd: false,
            rcmd: false,
            shift: false,
            ctrl: false,
            alt: false,
            swallow_up: None,
        }
    }

    /// Drop every key we believe is held and return to `Idle`, reporting the
    /// event the app needs to stay in sync.
    ///
    /// Called when the event tap is re-enabled after the system disabled it:
    /// transitions during the outage were never delivered, so our view of the
    /// keyboard is stale. Without this, a ⌘ key-up dropped mid-hold leaves the
    /// panel up with no key left that can close it, and hold activation dead
    /// until an unrelated ⌘ press releases it.
    ///
    /// A chord-shown panel is *not* dismissed — it is meant to outlive the
    /// keypress, and the phase is `Idle` there anyway.
    pub fn reset(&mut self) -> Option<Event> {
        let was_hold = self.phase == Phase::HoldActive;
        *self = Self::new();
        was_hold.then_some(Event::HoldReleased)
    }

    fn cmd_down(&self) -> bool {
        self.lcmd || self.rcmd
    }

    /// A ⌘ key-up that leaves no ⌘ still held — the real end of the press.
    /// Call after `track_modifier` has applied this transition.
    fn cmd_fully_released(&self, vk: u32, up: bool) -> bool {
        up && is_cmd(vk) && !self.cmd_down()
    }

    fn track_modifier(&mut self, vk: u32, down: bool) {
        match vk {
            0x10 | 0xA0 | 0xA1 => self.shift = down,
            0x11 | 0xA2 | 0xA3 => self.ctrl = down,
            0x12 | 0xA4 | 0xA5 => self.alt = down,
            VK_LWIN => self.lcmd = down,
            VK_RWIN => self.rcmd = down,
            _ => {}
        }
    }

    fn chord_matches(&self, cfg: &Config, vk: u32) -> bool {
        cfg.chord_enabled
            && vk == cfg.chord_vk
            && self.cmd_down() == cfg.chord_win
            && self.shift == cfg.chord_shift
            && self.ctrl == cfg.chord_ctrl
            && self.alt == cfg.chord_alt
    }

    /// One key transition (already translated to a VK). Modifier transitions
    /// come from flagsChanged, everything else from keyDown/keyUp; autorepeat
    /// arrives as repeated `down = true`.
    pub fn on_key(&mut self, cfg: &Config, vk: u32, down: bool) -> Decision {
        self.track_modifier(vk, down);
        let up = !down;

        // Swallow the matching key-up of a previously swallowed key.
        if up {
            if let Some(target) = self.swallow_up {
                if target == vk {
                    self.swallow_up = None;
                    return Decision { swallow: true, ..Decision::PASS };
                }
            }
        }

        // --- Chord (checked before phase logic) ---
        if down && !is_modifier(vk) && self.chord_matches(cfg, vk) {
            // Autorepeat of a still-held chord key: swallow without
            // re-emitting, else one press toggles several times.
            if self.swallow_up == Some(vk) {
                return Decision { swallow: true, ..Decision::PASS };
            }
            self.swallow_up = Some(vk);
            // Toggle: the overlay-visible flag tracks "visible and focused"
            // (the app clears it on focus loss), so chord-while-set means
            // dismiss and chord-while-clear means show/refocus — the app
            // resolves which.
            let event = if cfg.overlay_visible {
                Event::DismissedChordAgain
            } else {
                Event::ChordActivated
            };
            return Decision { swallow: true, event: Some(event), timer: None };
        }

        // --- Esc dismisses while the overlay is up (swallow down and up) ---
        if down && vk == VK_ESCAPE && cfg.overlay_visible {
            self.swallow_up = Some(VK_ESCAPE);
            return Decision {
                swallow: true,
                event: Some(Event::DismissedEsc),
                timer: None,
            };
        }

        match self.phase {
            Phase::Idle => {
                if down && is_cmd(vk) {
                    self.phase = Phase::CmdDown;
                    if cfg.hold_enabled {
                        return Decision {
                            timer: Some(TimerCmd::Arm(cfg.hold_ms)),
                            ..Decision::PASS
                        };
                    }
                }
                Decision::PASS
            }
            Phase::CmdDown => {
                if self.cmd_fully_released(vk, up) {
                    // Quick ⌘ tap: nothing to suppress on macOS.
                    self.phase = Phase::Idle;
                    Decision { timer: Some(TimerCmd::Cancel), ..Decision::PASS }
                } else if down && !is_cmd(vk) {
                    // Any other key -> real combo (⌘Tab, ⌘C...), cancel hold.
                    self.phase = Phase::Combo;
                    Decision { timer: Some(TimerCmd::Cancel), ..Decision::PASS }
                } else {
                    Decision::PASS
                }
            }
            Phase::HoldActive => {
                if self.cmd_fully_released(vk, up) {
                    self.phase = Phase::Idle;
                    Decision {
                        event: Some(Event::HoldReleased),
                        ..Decision::PASS
                    }
                } else if down && !is_modifier(vk) {
                    // ⌘-something while the panel is up: pass it through to
                    // the OS, dismiss, and become a combo. Lone modifiers do
                    // not dismiss (they trigger nothing until a key follows).
                    self.phase = Phase::Combo;
                    Decision {
                        event: Some(Event::DismissedPassthrough),
                        ..Decision::PASS
                    }
                } else {
                    Decision::PASS
                }
            }
            Phase::Combo => {
                if self.cmd_fully_released(vk, up) {
                    self.phase = Phase::Idle;
                }
                Decision::PASS
            }
        }
    }

    /// The hold timer elapsed. `foreground_excluded` is resolved by the
    /// adapter (is the frontmost bundle id on the exclusion list?) at fire
    /// time, mirroring the Windows agent.
    pub fn on_hold_timer(&mut self, cfg: &Config, foreground_excluded: bool) -> Decision {
        if self.phase != Phase::CmdDown {
            return Decision::PASS;
        }
        if self.shift || self.ctrl || self.alt {
            // A combo is in progress (⇧⌘4 pressed shift-first etc.); leave
            // it entirely to the OS.
            self.phase = Phase::Combo;
            return Decision::PASS;
        }
        if !cfg.hold_enabled || foreground_excluded {
            // Disabled, or an excluded app is frontmost: leave the ⌘ press
            // alone; the user can still combo/tap.
            return Decision::PASS;
        }
        self.phase = Phase::HoldActive;
        Decision {
            event: Some(Event::HoldActivated),
            ..Decision::PASS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VK_TAB: u32 = 0x09;
    const VK_SLASH: u32 = 0xBF; // VK_OEM_2

    /// ⌃⌘/ — the macOS default chord.
    fn cfg(visible: bool) -> Config {
        Config {
            hold_enabled: true,
            hold_ms: 900,
            chord_enabled: true,
            chord_win: true,
            chord_ctrl: true,
            chord_shift: false,
            chord_alt: false,
            chord_vk: VK_SLASH,
            overlay_visible: visible,
        }
    }

    #[test]
    fn holding_cmd_past_threshold_fires_hold_activated() {
        let mut a = Activation::new();
        let c = cfg(false);
        let d = a.on_key(&c, VK_LWIN, true);
        assert!(!d.swallow);
        assert_eq!(d.timer, Some(TimerCmd::Arm(900)));
        assert_eq!(d.event, None);
        let d = a.on_hold_timer(&c, false);
        assert_eq!(d.event, Some(Event::HoldActivated));
        // Releasing afterwards reports the release.
        let d = a.on_key(&c, VK_LWIN, false);
        assert_eq!(d.event, Some(Event::HoldReleased));
        assert!(!d.swallow);
    }

    #[test]
    fn cmd_tab_drops_to_combo_and_never_holds() {
        let mut a = Activation::new();
        let c = cfg(false);
        assert_eq!(a.on_key(&c, VK_LWIN, true).timer, Some(TimerCmd::Arm(900)));
        let d = a.on_key(&c, VK_TAB, true);
        assert!(!d.swallow);
        assert_eq!(d.timer, Some(TimerCmd::Cancel));
        // A late timer fire (adapter raced the cancel) must not activate.
        assert_eq!(a.on_hold_timer(&c, false), Decision::PASS);
        assert_eq!(a.on_key(&c, VK_TAB, false), Decision::PASS);
        let d = a.on_key(&c, VK_LWIN, false);
        assert_eq!(d.event, None);
    }

    #[test]
    fn quick_cmd_tap_fires_nothing() {
        let mut a = Activation::new();
        let c = cfg(false);
        a.on_key(&c, VK_LWIN, true);
        let d = a.on_key(&c, VK_LWIN, false);
        assert!(!d.swallow);
        assert_eq!(d.event, None);
        assert_eq!(d.timer, Some(TimerCmd::Cancel));
    }

    #[test]
    fn chord_fires_and_swallows_down_and_up() {
        let mut a = Activation::new();
        let c = cfg(false);
        a.on_key(&c, VK_LWIN, true);
        a.on_key(&c, 0xA2, true); // left ctrl
        let d = a.on_key(&c, VK_SLASH, true);
        assert!(d.swallow);
        assert_eq!(d.event, Some(Event::ChordActivated));
        // Autorepeat while held: swallowed, no re-emit.
        let d = a.on_key(&c, VK_SLASH, true);
        assert!(d.swallow);
        assert_eq!(d.event, None);
        let d = a.on_key(&c, VK_SLASH, false);
        assert!(d.swallow);
        assert_eq!(d.event, None);
    }

    #[test]
    fn chord_again_while_visible_dismisses() {
        let mut a = Activation::new();
        let c = cfg(true);
        a.on_key(&c, VK_LWIN, true);
        a.on_key(&c, 0xA2, true);
        let d = a.on_key(&c, VK_SLASH, true);
        assert!(d.swallow);
        assert_eq!(d.event, Some(Event::DismissedChordAgain));
    }

    #[test]
    fn esc_while_visible_dismisses_and_is_swallowed() {
        let mut a = Activation::new();
        let c = cfg(true);
        let d = a.on_key(&c, VK_ESCAPE, true);
        assert!(d.swallow);
        assert_eq!(d.event, Some(Event::DismissedEsc));
        let d = a.on_key(&c, VK_ESCAPE, false);
        assert!(d.swallow);
        assert_eq!(d.event, None);
    }

    #[test]
    fn esc_while_not_visible_passes_through() {
        let mut a = Activation::new();
        let c = cfg(false);
        let d = a.on_key(&c, VK_ESCAPE, true);
        assert!(!d.swallow);
        assert_eq!(d.event, None);
        assert!(!a.on_key(&c, VK_ESCAPE, false).swallow);
    }

    #[test]
    fn excluded_foreground_prevents_hold() {
        let mut a = Activation::new();
        let c = cfg(false);
        a.on_key(&c, VK_LWIN, true);
        let d = a.on_hold_timer(&c, true);
        assert_eq!(d.event, None);
        // The ⌘ press stays live: the user can still combo out of it.
        let d = a.on_key(&c, VK_TAB, true);
        assert!(!d.swallow);
        let d = a.on_key(&c, VK_LWIN, false);
        assert_eq!(d.event, None);
    }

    #[test]
    fn modifier_held_at_threshold_means_combo() {
        let mut a = Activation::new();
        let c = cfg(false);
        // ⇧ then ⌘ held (screenshot-style chord in progress): no hold.
        a.on_key(&c, 0xA0, true);
        a.on_key(&c, VK_LWIN, true);
        assert_eq!(a.on_hold_timer(&c, false), Decision::PASS);
        // And a passthrough key while in Combo stays untouched.
        assert_eq!(a.on_key(&c, 0x34, true), Decision::PASS);
    }

    #[test]
    fn passthrough_while_hold_active_dismisses() {
        let mut a = Activation::new();
        let c = cfg(false);
        a.on_key(&c, VK_LWIN, true);
        a.on_hold_timer(&c, false);
        // ⌘E while the panel is up: pass through + dismiss.
        let d = a.on_key(&c, 0x45, true);
        assert!(!d.swallow);
        assert_eq!(d.event, Some(Event::DismissedPassthrough));
        // ⌘ release out of Combo ends quietly.
        assert_eq!(a.on_key(&c, 0x45, false), Decision::PASS);
        let d = a.on_key(&c, VK_LWIN, false);
        assert_eq!(d.event, None);
    }

    /// Regression: the tap can be disabled mid-hold (timeout, sleep/wake), and
    /// the ⌘ key-up that happens while it is dead is never delivered. Without
    /// a reset on re-enable the panel stays up and hold activation is dead —
    /// the next ⌘ press re-arms nothing, because `HoldActive` ignores a ⌘ down.
    #[test]
    fn reset_on_tap_reenable_clears_a_wedged_hold() {
        let mut a = Activation::new();
        let c = cfg(false);
        a.on_key(&c, VK_LWIN, true);
        assert_eq!(a.on_hold_timer(&c, false).event, Some(Event::HoldActivated));

        // Tap dies here; the ⌘ up is swallowed by the outage. Re-enabling it
        // must tell the app the hold is over.
        assert_eq!(a.reset(), Some(Event::HoldReleased));

        // And the state machine is live again: the next ⌘ press arms and holds.
        let d = a.on_key(&c, VK_LWIN, true);
        assert_eq!(d.timer, Some(TimerCmd::Arm(900)));
        assert_eq!(a.on_hold_timer(&c, false).event, Some(Event::HoldActivated));
    }

    /// Resetting outside a hold reports nothing — a chord-shown panel is meant
    /// to outlive the keypress and must not be dismissed by tap recovery.
    #[test]
    fn reset_outside_a_hold_is_silent() {
        let mut a = Activation::new();
        let c = cfg(true);
        assert_eq!(Activation::new().reset(), None);
        a.on_key(&c, VK_LWIN, true); // CmdDown, timer armed
        assert_eq!(a.reset(), None);
        // The stale timer fire that outlived the reset must not activate.
        assert_eq!(a.on_hold_timer(&c, false), Decision::PASS);
    }

    /// Both ⌘ keys held: releasing one is not the end of the press.
    #[test]
    fn releasing_one_cmd_while_the_other_is_held_keeps_the_hold() {
        let mut a = Activation::new();
        let c = cfg(false);
        a.on_key(&c, VK_LWIN, true);
        a.on_key(&c, VK_RWIN, true);
        assert_eq!(a.on_hold_timer(&c, false).event, Some(Event::HoldActivated));
        // Left ⌘ up, right still down: the panel stays.
        assert_eq!(a.on_key(&c, VK_LWIN, false).event, None);
        // The last ⌘ up ends it.
        assert_eq!(a.on_key(&c, VK_RWIN, false).event, Some(Event::HoldReleased));
    }

    #[test]
    fn hold_disabled_never_arms() {
        let mut a = Activation::new();
        let mut c = cfg(false);
        c.hold_enabled = false;
        let d = a.on_key(&c, VK_LWIN, true);
        assert_eq!(d.timer, None);
        assert_eq!(a.on_hold_timer(&c, false).event, None);
    }
}
