import type { Appearance, Theme } from "./types";

export function applyTheme(theme: Theme) {
  const resolved =
    theme === "system"
      ? window.matchMedia?.("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light"
      : theme;
  document.documentElement.dataset.theme = resolved;
}

/** Apply theme + panel opacity (drives the --panel-alpha CSS variable).
 * `scaleFont` additionally drives the root font-size, which every rem-based
 * size (text and keycaps together) follows — the overlay window opts in;
 * settings and badges stay at 100% (ADR 0005). */
export function applyAppearance(a: Appearance, scaleFont = false) {
  applyTheme(a.theme);
  document.documentElement.style.setProperty(
    "--panel-alpha",
    String(Math.min(1, Math.max(0, a.panelOpacity))),
  );
  if (scaleFont) {
    document.documentElement.style.fontSize = `${(a.fontScale ?? 1) * 100}%`;
  }
}
