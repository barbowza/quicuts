// Renders the PTSG `Keys` grammar into keycap labels. Mirrors PowerToys'
// KeyVisual: modifiers render as leading keycaps, then each key token maps to
// a label or glyph. Platform switches the modifier glyphs (Win vs mac).

import type { Key, KeyCombo, GlyphToken } from "../lib/types";

export type Platform = "windows" | "macos";

/** Keys drawn as SVG icons (KeyIcon.svelte) instead of a Unicode label —
 *  the font glyphs (⊞ ⇧ ← ↵ ⌫ …) are thin and small at keycap sizes. */
export type KeyIconName =
  | "win"
  | "shift"
  | "up"
  | "down"
  | "left"
  | "right"
  | "arrow"
  | "arrowLr"
  | "arrowUd"
  | "enter"
  | "backspace";

export interface Cap {
  label: string;
  /** Typeable words for the filter box when the label is a glyph ("win",
   *  "shift", "enter") — a user cannot type ⊞ or ⇧. */
  search?: string;
  /** underline the single letter (for <Underlined letter>) */
  underline?: boolean;
  wide?: boolean;
  /** Render this icon instead of the label (which stays for filtering). */
  icon?: KeyIconName;
}

// Filter aliases for tokens whose GLYPHS entry is untypeable; word-labeled
// glyphs ("Esc", "Page Up", ...) already match as typed.
const GLYPH_SEARCH: Partial<Record<GlyphToken, string>> = {
  left: "left",
  right: "right",
  up: "up",
  down: "down",
  arrow: "arrow",
  arrow_l_r: "arrow",
  arrow_u_d: "arrow",
  enter: "enter",
  backspace: "backspace",
};

const GLYPHS: Record<GlyphToken, string> = {
  left: "←",
  right: "→",
  up: "↑",
  down: "↓",
  arrow: "↔",
  arrow_l_r: "↔",
  arrow_u_d: "↕",
  enter: "↵",
  backspace: "⌫",
  escape: "Esc",
  page_up: "Page Up",
  page_down: "Page Down",
  home: "Home",
  end: "End",
  insert: "Insert",
  delete: "Delete",
  pause: "Pause",
  prt_scr: "PrtScr",
  copilot: "Copilot",
  office: "Office",
};

// VK -> label for the named keys; letters, digits, numpad and function keys
// are covered as ranges in vkLabel(). Unknown codes fall back to a VK badge.
const VK_LABELS: Record<number, string> = {
  0x08: "⌫",
  0x09: "Tab",
  0x0d: "↵",
  0x10: "⇧",
  0x11: "Ctrl",
  0x12: "Alt",
  0x13: "Pause",
  0x14: "Caps",
  0x1b: "Esc",
  0x20: "Space",
  0x21: "PgUp",
  0x22: "PgDn",
  0x23: "End",
  0x24: "Home",
  0x25: "←",
  0x26: "↑",
  0x27: "→",
  0x28: "↓",
  0x2c: "PrtScr",
  0x2d: "Insert",
  0x2e: "Delete",
  0x5b: "WIN", // 91 decimal — PTSG's Windows key in populated entries
  0x5c: "WIN",
  0x5d: "Menu",
  0x6a: "Num *",
  0x6b: "Num +",
  0x6d: "Num -",
  0x6e: "Num .",
  0x6f: "Num /",
  0x90: "NumLock",
  // OEM punctuation (US layout) — also used to render configured chords
  // like the settings chord (Ctrl+, = VK 0xBC).
  0xba: ";",
  0xbb: "=",
  0xbc: ",",
  0xbd: "-",
  0xbe: ".",
  0xbf: "/",
  0xc0: "`",
  0xdb: "[",
  0xdc: "\\",
  0xdd: "]",
  0xde: "'",
};

// Filter aliases for the glyph-labeled VK entries above.
const VK_SEARCH: Record<number, string> = {
  0x08: "backspace",
  0x0d: "enter",
  0x25: "left",
  0x26: "up",
  0x27: "right",
  0x28: "down",
};

const GLYPH_ICONS: Partial<Record<GlyphToken, KeyIconName>> = {
  left: "left",
  right: "right",
  up: "up",
  down: "down",
  arrow: "arrow",
  arrow_l_r: "arrowLr",
  arrow_u_d: "arrowUd",
  enter: "enter",
  backspace: "backspace",
};

const VK_ICONS: Record<number, KeyIconName> = {
  0x08: "backspace",
  0x0d: "enter",
  0x10: "shift",
  0x25: "left",
  0x26: "up",
  0x27: "right",
  0x28: "down",
};

function vkLabel(vk: number): string | undefined {
  // Bare "0"-"9" manifest tokens parse as VK 0-9; render them back as digits.
  // This wins over the VK 8/9 (Backspace/Tab) reading — manifests spell those out.
  if (vk <= 9) return String(vk);
  // The character ranges, same as PTSG's KeyVisual.
  if (vk >= 0x30 && vk <= 0x39) return String.fromCharCode(vk); // 0-9
  if (vk >= 0x41 && vk <= 0x5a) return String.fromCharCode(vk); // A-Z
  if (vk >= 0x60 && vk <= 0x69) return `Num ${vk - 0x60}`;
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`;
  return VK_LABELS[vk];
}

function winModCap(kind: "win" | "ctrl" | "alt" | "shift", platform: Platform): Cap {
  if (platform === "macos") {
    return { label: { win: "⌘", ctrl: "⌃", alt: "⌥", shift: "⇧" }[kind], search: kind };
  }
  const label = { win: "⊞", ctrl: "Ctrl", alt: "Alt", shift: "⇧" }[kind];
  const icon = kind === "win" ? ("win" as const) : kind === "shift" ? ("shift" as const) : undefined;
  return { label, search: kind, icon };
}

function keyToCap(key: Key, platform: Platform): Cap {
  switch (key.kind) {
    case "literal":
      return { label: key.value };
    case "angle_literal":
      return { label: key.value };
    case "glyph":
      return {
        label: GLYPHS[key.value] ?? key.value,
        search: GLYPH_SEARCH[key.value],
        icon: GLYPH_ICONS[key.value],
      };
    case "underlined_letter":
      return { label: "", underline: true };
    case "taskbar_range":
      return { label: "Num" };
    case "vk": {
      const known = vkLabel(key.value);
      if (known === "WIN") return winModCap("win", platform);
      if (known) return { label: known, search: VK_SEARCH[key.value], icon: VK_ICONS[key.value] };
      return { label: `VK${key.value}` };
    }
  }
}

/** Ordered keycaps for one combo: modifiers first (Win, Ctrl, Alt, Shift). */
export function comboToCaps(combo: KeyCombo, platform: Platform): Cap[] {
  const caps: Cap[] = [];
  if (combo.win) caps.push(winModCap("win", platform));
  if (combo.ctrl) caps.push(winModCap("ctrl", platform));
  if (combo.alt) caps.push(winModCap("alt", platform));
  if (combo.shift) caps.push(winModCap("shift", platform));
  for (const k of combo.keys) caps.push(keyToCap(k, platform));
  return caps;
}

// --- Display bindings --------------------------------------------------------
// A "binding" is one displayable way to trigger the shortcut: either a single
// chord or a chord sequence. Defaults come from the manifest (green caps,
// mid-gray when the user marked them reassigned); customs come from the
// user's customization file (yellow caps) and always stack on top.

import type { AssembledEntry, ComboDisplayMode } from "../lib/types";

export type BindingKind = "default" | "custom" | "redefined";

export interface Binding {
  kind: BindingKind;
  chords: KeyCombo[];
  sequence: boolean;
  /** Stored combo string — identity for removing a custom binding. */
  raw?: string;
}

/** The manifest's default combos grouped into bindings (one sequence, or one
 *  binding per alternative), each flagged redefined when the user marked it. */
export function defaultBindings(entry: AssembledEntry, isSequence: boolean): Binding[] {
  const redefined = entry.redefined ?? [];
  if (entry.combos.length === 0) return [];
  if (isSequence) {
    const all = redefined.length > 0 && entry.combos.every((_, i) => redefined[i]);
    return [{ kind: all ? "redefined" : "default", chords: entry.combos, sequence: true }];
  }
  return entry.combos.map((c, i) => ({
    kind: redefined[i] ? "redefined" : "default",
    chords: [c],
    sequence: false,
  }));
}

export function customBindings(entry: AssembledEntry): Binding[] {
  return (entry.customCombos ?? []).map((c) => ({
    kind: "custom",
    chords: c.chords,
    sequence: c.chords.length > 1,
    raw: c.raw,
  }));
}

/** All bindings to display for one entry under the given display mode:
 *  custom on top, defaults (incl. redefined) below. */
export function entryBindings(
  entry: AssembledEntry,
  isSequence: boolean,
  mode: ComboDisplayMode,
): Binding[] {
  const customs = customBindings(entry);
  const defaults = defaultBindings(entry, isSequence);
  switch (mode) {
    case "default":
      return defaults;
    case "custom":
      return customs;
    case "customElseDefault":
      return customs.length > 0 ? customs : defaults;
    default:
      return [...customs, ...defaults];
  }
}

// --- Key capture ---------------------------------------------------------------

// Punctuation by physical key, so Ctrl+Shift+1 captures "1" rather than "!".
const CODE_PUNCT: Record<string, string> = {
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  BracketLeft: "[",
  BracketRight: "]",
  Minus: "-",
  Equal: "=",
  Backquote: "`",
};

const KEY_GLYPHS: Record<string, GlyphToken> = {
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  Enter: "enter",
  Backspace: "backspace",
  Escape: "escape",
  PageUp: "page_up",
  PageDown: "page_down",
  Home: "home",
  End: "end",
  Insert: "insert",
  Delete: "delete",
  Pause: "pause",
  PrintScreen: "prt_scr",
};

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "CapsLock", "NumLock", "ScrollLock", "Dead"]);

/** Map a captured keydown to a manifest Key; null for modifier-only events. */
export function eventToKey(e: KeyboardEvent): Key | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  const code = e.code;
  let m = /^Key([A-Z])$/.exec(code);
  if (m) return { kind: "literal", value: m[1] };
  m = /^(?:Digit|Numpad)(\d)$/.exec(code);
  if (m) return { kind: "literal", value: m[1] };
  if (/^F\d{1,2}$/.test(code)) return { kind: "literal", value: code };
  if (CODE_PUNCT[code]) return { kind: "literal", value: CODE_PUNCT[code] };
  if (KEY_GLYPHS[e.key]) return { kind: "glyph", value: KEY_GLYPHS[e.key] };
  if (e.key === " " || code === "Space") return { kind: "literal", value: "Space" };
  if (e.key === "Tab") return { kind: "literal", value: "Tab" };
  if (e.key.length === 1) return { kind: "literal", value: e.key.toUpperCase() };
  return { kind: "literal", value: e.key };
}

// --- Sequence detection -----------------------------------------------------
// The PTSG format uses one `Shortcut` list both for alternative bindings
// ("Copy: Ctrl+Shift+C or Ctrl+Insert") and for chord sequences ("VSCode:
// Ctrl+K, Ctrl+S") with no distinguishing flag. Heuristic that classifies the
// whole bundled set correctly: a sequence's chords share one modifier set,
// each press a single key, and the first chord is a *prefix key* — it opens
// several sequences on the same page (VSCode's Ctrl+K, tmux's Ctrl+B, ...).
// Alternatives never repeat a first chord across entries. Do NOT require
// later chords to share the prefix's modifiers: VSCode's "Toggle Zen Mode"
// is Ctrl+K then a bare Z.

import type { AssembledSection } from "../lib/types";

function comboSig(c: KeyCombo): string {
  return JSON.stringify([c.win, c.ctrl, c.shift, c.alt, c.keys]);
}

/** Entry keys (per page) whose combos form a chord sequence, not alternatives. */
export function sequenceEntryKeys(sections: AssembledSection[]): Set<string> {
  const candidates: { key: string; dedup: string; first: string }[] = [];
  for (const s of sections) {
    for (const e of s.entries) {
      if (e.combos.length < 2) continue;
      const [f] = e.combos;
      const singleKeys = e.combos.every((c) => c.keys.length === 1);
      const prefixHasMods = f.win || f.ctrl || f.shift || f.alt;
      if (!singleKeys || !prefixHasMods) continue;
      candidates.push({
        key: e.key,
        // The same shortcut can appear under Pinned/Recommended and its own
        // category; count it once when tallying prefix usage.
        dedup: e.name + e.combos.map(comboSig).join("|"),
        first: comboSig(f),
      });
    }
  }
  const firstCount = new Map<string, number>();
  const seen = new Set<string>();
  for (const c of candidates) {
    if (seen.has(c.dedup)) continue;
    seen.add(c.dedup);
    firstCount.set(c.first, (firstCount.get(c.first) ?? 0) + 1);
  }
  const out = new Set<string>();
  for (const c of candidates) {
    if ((firstCount.get(c.first) ?? 0) >= 2) out.add(c.key);
  }
  return out;
}
