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

/** Apply theme + panel opacity (drives the --panel-alpha CSS variable). */
export function applyAppearance(a: Appearance) {
  applyTheme(a.theme);
  document.documentElement.style.setProperty(
    "--panel-alpha",
    String(Math.min(1, Math.max(0, a.panelOpacity))),
  );
}
