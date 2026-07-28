// Thin wrappers over Tauri's event/invoke bridge. Falls back to no-ops when
// run in a plain browser (Vite dev without the Tauri host).

import type { Appearance, KeyCombo, OverlayState } from "./types";

type Unlisten = () => void;

async function tauri() {
  try {
    return await import("@tauri-apps/api/event");
  } catch {
    return null;
  }
}

async function core() {
  try {
    return await import("@tauri-apps/api/core");
  } catch {
    return null;
  }
}

export async function onOverlayState(cb: (s: OverlayState) => void): Promise<Unlisten> {
  const ev = await tauri();
  if (!ev) return () => {};
  return ev.listen<OverlayState>("overlay://state", (e) => cb(e.payload));
}

export async function onAppearance(cb: (a: Appearance) => void): Promise<Unlisten> {
  const ev = await tauri();
  if (!ev) return () => {};
  return ev.listen<Appearance>("appearance://changed", (e) => cb(e.payload));
}

export async function onClearFilter(cb: () => void): Promise<Unlisten> {
  const ev = await tauri();
  if (!ev) return () => {};
  return ev.listen("overlay://clear-filter", () => cb());
}

/// Esc pressed while a customization dialog / key capture is open (the agent
/// swallows Esc, so the host routes it here as an event).
export async function onModalEsc(cb: () => void): Promise<Unlisten> {
  const ev = await tauri();
  if (!ev) return () => {};
  return ev.listen("overlay://modal-esc", () => cb());
}

export async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  const c = await core();
  if (!c) return null;
  return c.invoke<T>(cmd, args);
}

/// Pull the current theme from persisted settings; windows call this on
/// mount so they don't depend on a theme://changed push having happened.
export async function getAppearance(): Promise<Appearance | null> {
  // May run while the app is still initializing (hidden windows load early);
  // treat a failed invoke as "unknown" — the host re-pushes appearance on show.
  const s = await invokeCmd<{ appearance?: Appearance }>("get_settings").catch(() => null);
  if (!s?.appearance?.theme) return null;
  return {
    theme: s.appearance.theme,
    panelOpacity: s.appearance.panelOpacity ?? 0.82,
    fontScale: s.appearance.fontScale ?? 1,
  };
}

export const hideOverlay = () => invokeCmd("hide_overlay");
export const dismissOverlay = () => invokeCmd("dismiss_overlay");
export const setFilterActive = (active: boolean) => invokeCmd("set_filter_active", { active });
export const openSettings = () => invokeCmd("open_settings");
export const selectApp = (manifestId: string) => invokeCmd("select_app", { manifestId });
export const setPinnedApp = (manifestId: string | null) =>
  invokeCmd("set_pinned_app", { manifestId });
export const togglePin = (manifestId: string, entryKey: string) =>
  invokeCmd("toggle_pin", { manifestId, entryKey });
export const setOverlayModal = (modal: "none" | "dialog" | "capture") =>
  invokeCmd("set_overlay_modal", { modal });
export const addCustomization = (manifestId: string, entryName: string, chords: KeyCombo[]) =>
  invokeCmd("add_customization", { manifestId, entryName, chords });
export const removeCustomization = (manifestId: string, entryName: string, combo: string) =>
  invokeCmd("remove_customization", { manifestId, entryName, combo });
export const setDefaultRedefined = (
  manifestId: string,
  entryName: string,
  chords: KeyCombo[],
  redefined: boolean,
) => invokeCmd("set_default_redefined", { manifestId, entryName, chords, redefined });
export const setComboDisplayMode = (mode: string) =>
  invokeCmd("set_combo_display_mode", { mode });
export const adjustFontScale = (action: "increase" | "decrease" | "reset") =>
  invokeCmd("adjust_font_scale", { action });
