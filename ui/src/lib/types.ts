// TypeScript mirror of the Rust view-model types emitted by quicuts-app.
// The `Key` shape matches quicuts_manifest::Key's serde output, e.g.
//   {"kind":"literal","value":"A"}
//   {"kind":"vk","value":91}
//   {"kind":"glyph","value":"arrow_l_r"}
//   {"kind":"underlined_letter"} | {"kind":"taskbar_range"}
//   {"kind":"angle_literal","value":"Space"}

export type GlyphToken =
  | "left" | "right" | "up" | "down"
  | "arrow" | "arrow_l_r" | "arrow_u_d"
  | "enter" | "backspace" | "escape"
  | "page_up" | "page_down" | "home" | "end"
  | "insert" | "delete" | "pause" | "prt_scr"
  | "copilot" | "office";

export type Key =
  | { kind: "literal"; value: string }
  | { kind: "vk"; value: number }
  | { kind: "glyph"; value: GlyphToken }
  | { kind: "underlined_letter" }
  | { kind: "taskbar_range" }
  | { kind: "angle_literal"; value: string };

export interface KeyCombo {
  win: boolean;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  keys: Key[];
}

export type SectionKind = "pinned" | "recommended" | "category" | "taskbar";

/** One user-added binding: the stored string plus its parsed chords. */
export interface CustomBinding {
  raw: string;
  chords: KeyCombo[];
}

export interface AssembledEntry {
  key: string;
  name: string;
  description: string | null;
  pinned: boolean;
  combos: KeyCombo[];
  /** User-added bindings from the app's customization file. */
  customCombos: CustomBinding[];
  /** Parallel to combos: chord belongs to a reassigned default binding. */
  redefined: boolean[];
}

export interface AssembledSection {
  title: string;
  kind: SectionKind;
  entries: AssembledEntry[];
}

export interface RailApp {
  manifestId: string;
  displayName: string;
  iconUrl: string | null;
  isForeground: boolean;
  /** Placeholder for a foreground app with no collection (ADR 0004):
   *  rendered dimmed, not pinnable, page is empty. */
  unsupported: boolean;
}

export interface OverlayState {
  platform: "windows" | "macos";
  apps: RailApp[];
  /** manifestId -> assembled page */
  pages: Record<string, AssembledSection[]>;
  selected: string | null;
  /** Rail app pinned by the user (stays listed + selected), if any. */
  pinnedApp: string | null;
  /** Which combos to show; cycled by the panel's footer switch. */
  comboDisplayMode: ComboDisplayMode;
  /** Panel activation config, so Help can render the real bindings. */
  holdEnabled: boolean;
  chordEnabled: boolean;
  chord: ChordSpec;
  /** Chord that toggles the settings window while the panel has focus. */
  settingsChordEnabled: boolean;
  settingsChord: ChordSpec;
}

/** Mirror of quicuts_proto::ChordSpec (vk is a Windows virtual-key code). */
export interface ChordSpec {
  win: boolean;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  vk: number;
}

export type ComboDisplayMode = "default" | "custom" | "all" | "customElseDefault";

/** Mirror of quicuts_manifest::TitleBinding: a user-captured title
 * signature bound to a hosted collection (experimental title detection). */
export interface TitleBinding {
  pattern: string;
  manifestId: string;
}

/** One installed hosted collection, a valid target for a title binding. */
export interface HostedCollection {
  manifestId: string;
  displayName: string;
  /** The manifest's own TitleMatch patterns (for collision hints). */
  titleMatch: string[];
}

export type Theme = "system" | "light" | "dark";

/** Payload of appearance://changed — theme, panel opacity (0–1), and the
 * overlay font scale (0.8–2.0, ADR 0005). */
export interface Appearance {
  theme: Theme;
  panelOpacity: number;
  fontScale: number;
}
