// Chord capture and matching, shared by the settings window and the overlay.
//
// A chord's `vk` is a Windows virtual-key code on both platforms; the agent
// compares it to the key it sees. On Windows the webview's
// `KeyboardEvent.keyCode` *is* that VK, so capture stores it as is.
//
// On macOS it is not the same number (ADR 0006, chord-capture audit). WebKit
// derives `keyCode` from the typed *character* for letters and US
// punctuation (KeyEventMac.mm, windowsKeyCodeForCharCode). The mac agent
// translates the *physical* key by its US-ANSI position (tap.rs,
// vk_for_keycode). The two agree on a US layout and disagree everywhere
// else: on German QWERTZ the key labelled Z sits in the ANSI Y position, so
// WebKit says VK_Z and the agent says VK_Y, and a captured ⌃⌘Z never fires.
// WebKit also reports keys the agent cannot see at all (keypad, F13+), which
// would make a chord that silently never fires.
//
// So on macOS, capture and match by `KeyboardEvent.code` (physical
// position) through the agent's own table. That agrees with the agent on
// every layout, by construction.

export const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);

/** `KeyboardEvent.code` -> Windows VK, mirroring `vk_for_keycode` in
 * crates/quicuts-agent-mac/src/tap.rs (non-modifier keys). A key missing
 * here is one the agent never reports, so it cannot be a chord key. */
const MAC_CODE_VK: Record<string, number> = {
  Escape: 0x1b, Enter: 0x0d, Tab: 0x09, Space: 0x20, Backspace: 0x08, Delete: 0x2e,
  Home: 0x24, End: 0x23, PageUp: 0x21, PageDown: 0x22,
  ArrowLeft: 0x25, ArrowUp: 0x26, ArrowRight: 0x27, ArrowDown: 0x28,
  Semicolon: 0xba, Equal: 0xbb, Comma: 0xbc, Minus: 0xbd, Period: 0xbe, Slash: 0xbf,
  Backquote: 0xc0, BracketLeft: 0xdb, Backslash: 0xdc, BracketRight: 0xdd, Quote: 0xde,
};
for (let i = 0; i < 26; i++) MAC_CODE_VK[`Key${String.fromCharCode(65 + i)}`] = 0x41 + i;
for (let i = 0; i <= 9; i++) MAC_CODE_VK[`Digit${i}`] = 0x30 + i;
for (let i = 1; i <= 12; i++) MAC_CODE_VK[`F${i}`] = 0x6f + i;

/** The chord VK for a key event, or null when the agent could never
 * report that key on this platform. */
export function chordVk(e: KeyboardEvent): number | null {
  if (!isMac) return e.keyCode;
  return MAC_CODE_VK[e.code] ?? null;
}

export interface ChordSpec {
  win: boolean;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  vk: number;
}

/** True when `e` is chord `c` (same key, exactly the same modifiers). */
export function matchesChord(e: KeyboardEvent, c: ChordSpec): boolean {
  return (
    chordVk(e) === c.vk &&
    e.ctrlKey === c.ctrl &&
    e.shiftKey === c.shift &&
    e.altKey === c.alt &&
    e.metaKey === c.win
  );
}

/** Keycap label for a Windows VK, when known. On macOS letters, digits and
 * punctuation name the US-layout key in that *position*: what the agent
 * matches. */
export function vkName(vk: number): string | null {
  if (vk >= 0x41 && vk <= 0x5a) return String.fromCharCode(vk); // A-Z
  if (vk >= 0x30 && vk <= 0x39) return String.fromCharCode(vk); // 0-9
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`; // F1-F24
  const named: Record<number, string> = {
    0x08: isMac ? "Delete" : "Backspace", 0x09: "Tab", 0x0d: isMac ? "Return" : "Enter",
    0x1b: "Esc", 0x20: "Space",
    0x21: "Page Up", 0x22: "Page Down", 0x23: "End", 0x24: "Home",
    0x25: "←", 0x26: "↑", 0x27: "→", 0x28: "↓",
    0x2d: "Insert", 0x2e: isMac ? "⌦" : "Delete",
    0xba: ";", 0xbb: "=", 0xbc: ",", 0xbd: "-", 0xbe: ".", 0xbf: "/",
    0xc0: "`", 0xdb: "[", 0xdc: "\\", 0xdd: "]", 0xde: "'",
  };
  return named[vk] ?? null;
}

/** Human label: "Win + Shift + / (0xBF)" on Windows, "⌃⌘/ (0xBF)" on macOS
 * (Apple's modifier order: ⌃ ⌥ ⇧ ⌘). */
export function chordLabel(c: ChordSpec): string {
  const hex = `0x${c.vk.toString(16).toUpperCase()}`;
  const name = vkName(c.vk);
  const key = name ? `${name} (${hex})` : hex;
  if (isMac) {
    const mods = (c.ctrl ? "⌃" : "") + (c.alt ? "⌥" : "") + (c.shift ? "⇧" : "") + (c.win ? "⌘" : "");
    return mods + key;
  }
  const parts: string[] = [];
  if (c.win) parts.push("Win");
  if (c.ctrl) parts.push("Ctrl");
  if (c.alt) parts.push("Alt");
  if (c.shift) parts.push("Shift");
  parts.push(key);
  return parts.join(" + ");
}
