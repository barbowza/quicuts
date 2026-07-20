//! User shortcut customizations: per-app YAML files layered over the default
//! manifests without touching them. Quicuts only *documents* bindings — a
//! customization records what the user rebound in the app itself.
//!
//! File format (`{config}/customizations/<ManifestId>.custom.yml`):
//! ```yaml
//! Toggle Zen Mode:
//!   custom: ["Ctrl+Alt+Z"]
//!   redefined: ["Ctrl+K Z"]
//! ```
//! Combo strings are human-editable: chords separated by spaces, keys within
//! a chord joined by `+` ("Ctrl+K Z" = press Ctrl+K, then Z). `<...>` tokens
//! use the manifest grammar; `VKnn` is a raw virtual-key code; `Plus` (or
//! `++`) is the literal plus key. Entries are keyed by shortcut name, so a
//! customization survives manifest updates that only change the default keys.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::keys::{glyph_for, normalize_str, GlyphToken, Key};
use crate::KeyCombo;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntryCustomization {
    /// User-added bindings, in display order (combo-sequence strings).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<String>,
    /// Default bindings the user reassigned in the app (no longer available).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redefined: Vec<String>,
}

impl EntryCustomization {
    fn is_empty(&self) -> bool {
        self.custom.is_empty() && self.redefined.is_empty()
    }
}

/// All customizations for one app: shortcut name -> customization.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppCustomizations(pub BTreeMap<String, EntryCustomization>);

impl AppCustomizations {
    pub fn is_empty(&self) -> bool {
        self.0.values().all(EntryCustomization::is_empty)
    }

    pub fn get(&self, entry_name: &str) -> Option<&EntryCustomization> {
        self.0.get(entry_name)
    }

    /// Missing file -> empty; unparseable file -> logged, empty (never fatal,
    /// like manifest loading — the user may be mid-edit).
    pub fn load_file(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => serde_yaml_ng::from_str(&s).unwrap_or_else(|e| {
                log::warn!("customizations parse failed ({}): {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Atomic write (temp + rename); an empty store removes the file.
    pub fn save_file(&self, path: &Path) -> std::io::Result<()> {
        if self.is_empty() {
            return match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let yaml = serde_yaml_ng::to_string(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("yml.tmp");
        std::fs::write(&tmp, yaml)?;
        std::fs::rename(&tmp, path)
    }

    pub fn add_custom(&mut self, entry_name: &str, combo: &str) {
        let e = self.0.entry(entry_name.to_string()).or_default();
        if !e.custom.iter().any(|c| combo_eq(c, combo)) {
            e.custom.push(combo.to_string());
        }
    }

    pub fn remove_custom(&mut self, entry_name: &str, combo: &str) {
        if let Some(e) = self.0.get_mut(entry_name) {
            e.custom.retain(|c| !combo_eq(c, combo));
            if e.is_empty() {
                self.0.remove(entry_name);
            }
        }
    }

    pub fn set_redefined(&mut self, entry_name: &str, combo: &str, on: bool) {
        if on {
            let e = self.0.entry(entry_name.to_string()).or_default();
            if !e.redefined.iter().any(|r| combo_eq(r, combo)) {
                e.redefined.push(combo.to_string());
            }
        } else if let Some(e) = self.0.get_mut(entry_name) {
            e.redefined.retain(|r| !combo_eq(r, combo));
            if e.is_empty() {
                self.0.remove(entry_name);
            }
        }
    }
}

/// One parsed custom binding; `raw` is the stored string (its identity for
/// removal, and what a hand-editing user sees).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CustomBinding {
    pub raw: String,
    pub chords: Vec<KeyCombo>,
}

/// Parse the entry's custom strings, silently skipping unparseable ones
/// (the file is user-edited; bad lines must not take the page down).
pub fn custom_bindings(c: Option<&EntryCustomization>) -> Vec<CustomBinding> {
    c.map(|e| {
        e.custom
            .iter()
            .filter_map(|raw| {
                parse_combo_seq(raw).map(|chords| CustomBinding { raw: raw.clone(), chords })
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Flags parallel to `defaults`: true where a `redefined` string matches. A
/// multi-chord string marks its contiguous window (a whole sequence binding);
/// matching is semantic (case-insensitive, `Enter` == `<Enter>`), so hand
/// edits still line up.
pub fn mark_redefined(defaults: &[KeyCombo], redefined: &[String]) -> Vec<bool> {
    let mut flags = vec![false; defaults.len()];
    let def_norms: Vec<String> = defaults.iter().map(chord_norm).collect();
    for r in redefined {
        let Some(seq) = parse_combo_seq(r) else { continue };
        let norms: Vec<String> = seq.iter().map(chord_norm).collect();
        if norms.is_empty() || norms.len() > def_norms.len() {
            continue;
        }
        for start in 0..=(def_norms.len() - norms.len()) {
            if def_norms[start..start + norms.len()] == norms[..] {
                for f in &mut flags[start..start + norms.len()] {
                    *f = true;
                }
                break;
            }
        }
    }
    flags
}

// --- combo-string codec ------------------------------------------------------

/// "Ctrl+K Z" -> two chords; None if empty or any chord is malformed.
pub fn parse_combo_seq(s: &str) -> Option<Vec<KeyCombo>> {
    let toks = split_outside_angles(s, |c| c.is_whitespace(), false);
    if toks.is_empty() {
        return None;
    }
    toks.iter().map(|t| parse_chord(t)).collect()
}

fn parse_chord(tok: &str) -> Option<KeyCombo> {
    let parts = split_outside_angles(tok, |c| c == '+', true);
    if parts.is_empty() {
        return None;
    }
    let mut combo = KeyCombo { win: false, ctrl: false, shift: false, alt: false, keys: Vec::new() };
    for p in &parts {
        match p.to_ascii_lowercase().as_str() {
            "win" | "cmd" | "meta" | "super" => combo.win = true,
            "ctrl" | "control" => combo.ctrl = true,
            "shift" => combo.shift = true,
            "alt" | "option" | "opt" => combo.alt = true,
            _ => combo.keys.push(parse_key(p)),
        }
    }
    Some(combo)
}

/// Split on `sep` characters that sit outside `<...>` tokens. With
/// `plus_escape`, an empty segment keeps the separator itself as a token,
/// so "Ctrl++" reads as Ctrl + the literal plus key.
fn split_outside_angles(s: &str, sep: impl Fn(char) -> bool, plus_escape: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for c in s.chars() {
        if c == '<' {
            depth += 1;
            cur.push(c);
        } else if c == '>' {
            depth = (depth - 1).max(0);
            cur.push(c);
        } else if depth == 0 && sep(c) {
            if cur.is_empty() && plus_escape {
                cur.push(c);
            }
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_key(tok: &str) -> Key {
    if tok.starts_with('<') && tok.ends_with('>') && tok.len() > 2 {
        return normalize_str(tok);
    }
    if let Some(rest) = tok
        .strip_prefix("VK")
        .or_else(|| tok.strip_prefix("vk"))
        .or_else(|| tok.strip_prefix("Vk"))
    {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(n) = rest.parse() {
                return Key::Vk(n);
            }
        }
    }
    // Single characters are literal keycaps ("1" is the 1 key, never VK 1).
    let mut chars = tok.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Key::Literal(c.to_ascii_uppercase().to_string());
    }
    if tok.eq_ignore_ascii_case("esc") {
        return Key::Glyph(GlyphToken::Escape);
    }
    if tok.eq_ignore_ascii_case("return") {
        return Key::Glyph(GlyphToken::Enter);
    }
    if tok.eq_ignore_ascii_case("plus") {
        return Key::Literal("+".into());
    }
    if let Some(g) = glyph_for(tok) {
        return Key::Glyph(g);
    }
    Key::Literal(tok.to_string())
}

/// Canonical string for a chord sequence; round-trips through
/// `parse_combo_seq` to a semantically equal sequence.
pub fn format_combo_seq(chords: &[KeyCombo]) -> String {
    chords.iter().map(format_chord).collect::<Vec<_>>().join(" ")
}

fn format_chord(c: &KeyCombo) -> String {
    let mut parts: Vec<String> = Vec::new();
    if c.win {
        parts.push("Win".into());
    }
    if c.ctrl {
        parts.push("Ctrl".into());
    }
    if c.alt {
        parts.push("Alt".into());
    }
    if c.shift {
        parts.push("Shift".into());
    }
    for k in &c.keys {
        parts.push(format_key(k));
    }
    parts.join("+")
}

fn format_key(k: &Key) -> String {
    match k {
        Key::Literal(v) if v == "+" => "Plus".into(),
        // Tokens the splitter would break apart hide inside angle brackets.
        Key::Literal(v) if v.contains(['+', '<', '>']) || v.chars().any(char::is_whitespace) => {
            format!("<{v}>")
        }
        Key::Literal(v) => v.clone(),
        Key::Vk(n) => format!("VK{n}"),
        Key::Glyph(g) => glyph_name(*g).into(),
        Key::UnderlinedLetter => "<Underlined letter>".into(),
        Key::TaskbarRange => format!("<{}>", crate::keys::TASKBAR_TOKEN),
        Key::AngleLiteral(v) => format!("<{v}>"),
    }
}

fn glyph_name(g: GlyphToken) -> &'static str {
    match g {
        GlyphToken::Left => "Left",
        GlyphToken::Right => "Right",
        GlyphToken::Up => "Up",
        GlyphToken::Down => "Down",
        GlyphToken::Arrow => "Arrow",
        GlyphToken::ArrowLR => "ArrowLR",
        GlyphToken::ArrowUD => "ArrowUD",
        GlyphToken::Enter => "Enter",
        GlyphToken::Backspace => "Backspace",
        GlyphToken::Escape => "Escape",
        GlyphToken::PageUp => "PageUp",
        GlyphToken::PageDown => "PageDown",
        GlyphToken::Home => "Home",
        GlyphToken::End => "End",
        GlyphToken::Insert => "Insert",
        GlyphToken::Delete => "Delete",
        GlyphToken::Pause => "Pause",
        GlyphToken::PrtScr => "PrtScr",
        GlyphToken::Copilot => "Copilot",
        GlyphToken::Office => "Office",
    }
}

// --- semantic equality --------------------------------------------------------
// The same key can be spelled several ways (Literal("Enter"), <Enter> glyph,
// hand-typed lowercase); normalize before comparing so redefined markers and
// dedup survive spelling differences.

fn key_norm(k: &Key) -> String {
    match k {
        Key::Literal(v) | Key::AngleLiteral(v) => match glyph_for(v) {
            Some(g) => format!("g:{}", glyph_name(g)),
            None => format!("l:{}", v.to_ascii_lowercase()),
        },
        Key::Glyph(g) => format!("g:{}", glyph_name(*g)),
        Key::Vk(n) => format!("v:{n}"),
        Key::UnderlinedLetter => "u".into(),
        Key::TaskbarRange => "t".into(),
    }
}

fn chord_norm(c: &KeyCombo) -> String {
    let mods = [
        if c.win { "w" } else { "" },
        if c.ctrl { "c" } else { "" },
        if c.alt { "a" } else { "" },
        if c.shift { "s" } else { "" },
    ]
    .concat();
    let keys: Vec<String> = c.keys.iter().map(key_norm).collect();
    format!("{mods}|{}", keys.join(","))
}

/// Two combo strings are equal if identical, or both parse to the same
/// normalized chord sequence.
fn combo_eq(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (parse_combo_seq(a), parse_combo_seq(b)) {
        (Some(x), Some(y)) => {
            x.len() == y.len()
                && x.iter().zip(&y).all(|(cx, cy)| chord_norm(cx) == chord_norm(cy))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(win: bool, ctrl: bool, shift: bool, alt: bool, keys: Vec<Key>) -> KeyCombo {
        KeyCombo { win, ctrl, shift, alt, keys }
    }

    #[test]
    fn parses_single_chord() {
        let seq = parse_combo_seq("Ctrl+Alt+Z").unwrap();
        assert_eq!(seq, vec![chord(false, true, false, true, vec![Key::Literal("Z".into())])]);
    }

    #[test]
    fn parses_sequence_and_digits_as_literals() {
        let seq = parse_combo_seq("Ctrl+K 1").unwrap();
        assert_eq!(seq.len(), 2);
        assert_eq!(seq[1], chord(false, false, false, false, vec![Key::Literal("1".into())]));
    }

    #[test]
    fn parses_named_glyphs_vk_and_plus() {
        let seq = parse_combo_seq("Win+Up Ctrl+Plus Alt+VK120 Shift+<Enter>").unwrap();
        assert_eq!(seq[0].keys, vec![Key::Glyph(GlyphToken::Up)]);
        assert_eq!(seq[1].keys, vec![Key::Literal("+".into())]);
        assert_eq!(seq[2].keys, vec![Key::Vk(120)]);
        assert_eq!(seq[3].keys, vec![Key::Glyph(GlyphToken::Enter)]);
    }

    #[test]
    fn double_plus_is_literal_plus() {
        let seq = parse_combo_seq("Ctrl++").unwrap();
        assert_eq!(seq[0].keys, vec![Key::Literal("+".into())]);
        assert!(seq[0].ctrl);
    }

    #[test]
    fn format_round_trips_semantically() {
        for s in ["Ctrl+Alt+Z", "Ctrl+K Z", "Win+Shift+Left", "Ctrl+Plus", "Alt+VK123"] {
            let seq = parse_combo_seq(s).unwrap();
            let formatted = format_combo_seq(&seq);
            let reparsed = parse_combo_seq(&formatted).unwrap();
            assert!(
                seq.iter().map(chord_norm).eq(reparsed.iter().map(chord_norm)),
                "round-trip failed for {s} -> {formatted}"
            );
        }
    }

    #[test]
    fn literal_with_space_survives_round_trip() {
        let seq = vec![chord(false, true, false, false, vec![Key::Literal("Page Up".into())])];
        let formatted = format_combo_seq(&seq);
        let reparsed = parse_combo_seq(&formatted).unwrap();
        assert_eq!(reparsed.len(), 1);
        assert_eq!(chord_norm(&reparsed[0]), chord_norm(&seq[0]));
    }

    #[test]
    fn mark_redefined_alternative_and_sequence() {
        // Alternatives: Ctrl+C or Ctrl+Insert — mark only the second.
        let defaults = vec![
            chord(false, true, false, false, vec![Key::Literal("C".into())]),
            chord(false, true, false, false, vec![Key::Glyph(GlyphToken::Insert)]),
        ];
        let flags = mark_redefined(&defaults, &["Ctrl+Insert".to_string()]);
        assert_eq!(flags, vec![false, true]);

        // Sequence: Ctrl+K then Z — one string marks the whole window.
        let seq_defaults = vec![
            chord(false, true, false, false, vec![Key::Literal("K".into())]),
            chord(false, false, false, false, vec![Key::Literal("Z".into())]),
        ];
        let flags = mark_redefined(&seq_defaults, &["Ctrl+K Z".to_string()]);
        assert_eq!(flags, vec![true, true]);
    }

    #[test]
    fn mark_redefined_is_spelling_tolerant() {
        let defaults = vec![chord(false, true, false, false, vec![Key::Literal("Enter".into())])];
        assert_eq!(mark_redefined(&defaults, &["ctrl+<Enter>".to_string()]), vec![true]);
    }

    #[test]
    fn unparseable_custom_is_skipped() {
        let e = EntryCustomization {
            custom: vec!["Ctrl+A".into(), "   ".into()],
            redefined: vec![],
        };
        let bindings = custom_bindings(Some(&e));
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].raw, "Ctrl+A");
    }

    #[test]
    fn mutations_and_yaml_round_trip() {
        let mut c = AppCustomizations::default();
        c.add_custom("Copy", "Ctrl+Shift+C");
        c.add_custom("Copy", "ctrl+shift+c"); // semantic dup, ignored
        c.set_redefined("Copy", "Ctrl+C", true);
        assert_eq!(c.get("Copy").unwrap().custom.len(), 1);

        let yaml = serde_yaml_ng::to_string(&c).unwrap();
        let back: AppCustomizations = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(back, c);

        c.set_redefined("Copy", "ctrl+c", false); // spelling-tolerant removal
        c.remove_custom("Copy", "Ctrl+Shift+C");
        assert!(c.is_empty());
        assert!(c.get("Copy").is_none());
    }
}
