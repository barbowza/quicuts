//! Normalization of the PTSG `Keys` grammar into a typed model the UI can
//! render without re-parsing.
//!
//! PTSG semantics (from `ShortcutDescriptionToKeysConverter` + `KeyVisual`):
//! - plain strings render as-is on a keycap ("A", "Tab", "F4", "Space")
//! - multi-digit strings / ints are Windows virtual-key codes ("91" = Win);
//!   a single digit is the digit key itself ("0" in Ctrl+0)
//! - `<...>` tokens get special glyph rendering; *unknown* `<X>` renders the
//!   literal `X` (this is PTSG's escape hatch, e.g. `<0>` renders "0")

use serde::{Deserialize, Serialize};

use crate::schema::RawKeyToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlyphToken {
    Left,
    Right,
    Up,
    Down,
    Arrow,
    ArrowLR,
    ArrowUD,
    Enter,
    Backspace,
    Escape,
    PageUp,
    PageDown,
    Home,
    End,
    Insert,
    Delete,
    Pause,
    PrtScr,
    Copilot,
    Office,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Key {
    /// Render the string as-is on a keycap.
    Literal(String),
    /// Windows virtual-key code; UI maps to a label/glyph.
    Vk(u32),
    Glyph(GlyphToken),
    /// `<Underlined letter>` — keycap with underlined-letter styling.
    UnderlinedLetter,
    /// `<TASKBAR1-9>` — renders as a "Num" keycap; as a SectionName prefix it
    /// also arms the taskbar badge window.
    TaskbarRange,
    /// Unknown `<X>` token: render literal `X`.
    AngleLiteral(String),
}

pub(crate) fn glyph_for(inner: &str) -> Option<GlyphToken> {
    // Case-insensitive, whitespace-tolerant ("Page Up" == "PageUp").
    let folded: String = inner
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    Some(match folded.as_str() {
        "left" => GlyphToken::Left,
        "right" => GlyphToken::Right,
        "up" => GlyphToken::Up,
        "down" => GlyphToken::Down,
        "arrow" => GlyphToken::Arrow,
        "arrowlr" => GlyphToken::ArrowLR,
        "arrowud" => GlyphToken::ArrowUD,
        "enter" => GlyphToken::Enter,
        "backspace" => GlyphToken::Backspace,
        "escape" => GlyphToken::Escape,
        "pageup" => GlyphToken::PageUp,
        "pagedown" => GlyphToken::PageDown,
        "home" => GlyphToken::Home,
        "end" => GlyphToken::End,
        "insert" => GlyphToken::Insert,
        "delete" => GlyphToken::Delete,
        "pause" => GlyphToken::Pause,
        "prtscr" => GlyphToken::PrtScr,
        "copilot" => GlyphToken::Copilot,
        "office" => GlyphToken::Office,
        _ => return None,
    })
}

pub const TASKBAR_TOKEN: &str = "TASKBAR1-9";

pub fn normalize_token(raw: &RawKeyToken) -> Key {
    match raw {
        // A single digit is the digit *key* ("0" in Ctrl+0), not a VK code:
        // VK 0–9 are mouse buttons/control codes no manifest means, and PTSG
        // itself renders unmapped codes as the bare number, so the digit is
        // what shows either way. Multi-digit stays a VK code ("91" = Win).
        RawKeyToken::Num(n) if (0..=9).contains(n) => Key::Literal(n.to_string()),
        RawKeyToken::Num(n) => Key::Vk(*n as u32),
        RawKeyToken::Str(s) => normalize_str(s),
        RawKeyToken::Null => Key::Literal("~".into()),
    }
}

pub(crate) fn normalize_str(s: &str) -> Key {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('<').and_then(|r| r.strip_suffix('>')) {
        let inner = inner.trim();
        if inner.eq_ignore_ascii_case(TASKBAR_TOKEN) {
            return Key::TaskbarRange;
        }
        if inner.eq_ignore_ascii_case("underlined letter") {
            return Key::UnderlinedLetter;
        }
        if let Some(g) = glyph_for(inner) {
            return Key::Glyph(g);
        }
        return Key::AngleLiteral(inner.to_string());
    }
    // Same single-digit rule as `normalize_token` for quoted digits ("0").
    if s.len() > 1 && s.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(vk) = s.parse::<u32>() {
            return Key::Vk(vk);
        }
    }
    Key::Literal(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> RawKeyToken {
        RawKeyToken::Str(v.to_string())
    }

    #[test]
    fn plain_literals() {
        assert_eq!(normalize_token(&s("A")), Key::Literal("A".into()));
        assert_eq!(normalize_token(&s("F4")), Key::Literal("F4".into()));
        assert_eq!(normalize_token(&s("Tab")), Key::Literal("Tab".into()));
    }

    #[test]
    fn vk_codes_from_string_and_int() {
        assert_eq!(normalize_token(&s("91")), Key::Vk(91));
        assert_eq!(normalize_token(&RawKeyToken::Num(38)), Key::Vk(38));
    }

    #[test]
    fn single_digit_is_the_digit_key() {
        assert_eq!(normalize_token(&s("0")), Key::Literal("0".into()));
        assert_eq!(normalize_token(&s("9")), Key::Literal("9".into()));
        assert_eq!(normalize_token(&RawKeyToken::Num(0)), Key::Literal("0".into()));
        assert_eq!(normalize_token(&RawKeyToken::Num(7)), Key::Literal("7".into()));
    }

    #[test]
    fn glyph_tokens() {
        assert_eq!(normalize_token(&s("<Enter>")), Key::Glyph(GlyphToken::Enter));
        assert_eq!(normalize_token(&s("<Page Up>")), Key::Glyph(GlyphToken::PageUp));
        assert_eq!(normalize_token(&s("<escape>")), Key::Glyph(GlyphToken::Escape));
        assert_eq!(normalize_token(&s("<ArrowLR>")), Key::Glyph(GlyphToken::ArrowLR));
    }

    #[test]
    fn special_tokens() {
        assert_eq!(normalize_token(&s("<TASKBAR1-9>")), Key::TaskbarRange);
        assert_eq!(normalize_token(&s("<Underlined letter>")), Key::UnderlinedLetter);
    }

    #[test]
    fn unknown_angle_renders_literal() {
        assert_eq!(normalize_token(&s("<0>")), Key::AngleLiteral("0".into()));
        assert_eq!(normalize_token(&s("<Space>")), Key::AngleLiteral("Space".into()));
    }
}
