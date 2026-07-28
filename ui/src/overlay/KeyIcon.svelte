<script lang="ts">
  import type { KeyIconName } from "./keys";

  let { name }: { name: KeyIconName } = $props();

  // Solid glyphs drawn for small keycap sizes, where the Unicode characters
  // (⊞ ⇧ ← …) render thin and undersized. One arrow shape, rotated per
  // direction. Shift is deliberately wider and squatter than the up arrow
  // (full-width head, fat short stem) so the two stay distinct.
  const ARROW = "M12 2.5 L19 10.5 H15.2 V21.5 H8.8 V10.5 H5 Z";
  const PATHS: Record<KeyIconName, string> = {
    win:
      "M2 5.2 L10.9 4 V11.35 H2 Z M12.1 3.83 L22 2.5 V11.35 H12.1 Z " +
      "M2 12.65 H10.9 V20 L2 18.8 Z M12.1 12.65 H22 V21.5 L12.1 20.17 Z",
    shift: "M12 3 L22 12.8 H16.8 V20.8 H7.2 V12.8 H2 Z",
    up: ARROW,
    down: ARROW,
    left: ARROW,
    right: ARROW,
  };
  const ROTATE: Partial<Record<KeyIconName, number>> = { right: 90, down: 180, left: 270 };
  const rot = $derived(ROTATE[name]);
</script>

<svg viewBox="0 0 24 24" aria-hidden="true">
  <path d={PATHS[name]} transform={rot ? `rotate(${rot} 12 12)` : undefined} />
</svg>

<style>
  /* em-based so the icon tracks the keycap's font-size (and with it the
     accessibility font scale, ADR 0005). */
  svg {
    width: 1.2em;
    height: 1.2em;
    display: block;
    fill: currentColor;
  }
</style>
