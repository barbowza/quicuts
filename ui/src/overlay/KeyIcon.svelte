<script lang="ts">
  import type { KeyIconName } from "./keys";

  let { name }: { name: KeyIconName } = $props();

  // Solid glyphs drawn for small keycap sizes, where the Unicode characters
  // (⊞ ⇧ ← …) render thin and undersized. One arrow shape, rotated per
  // direction; the double/quad arrows share its head proportions. Shift is
  // deliberately wider and squatter than the up arrow (full-width head, fat
  // short stem) so the two stay distinct. Backspace is a solid tag with an
  // X knocked out (fill-rule evenodd).
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
    arrow:
      "M12 1.5 L15.4 5.8 L13.4 5.8 L13.4 10.6 L18.2 10.6 L18.2 8.6 L22.5 12 " +
      "L18.2 15.4 L18.2 13.4 L13.4 13.4 L13.4 18.2 L15.4 18.2 L12 22.5 " +
      "L8.6 18.2 L10.6 18.2 L10.6 13.4 L5.8 13.4 L5.8 15.4 L1.5 12 " +
      "L5.8 8.6 L5.8 10.6 L10.6 10.6 L10.6 5.8 L8.6 5.8 Z",
    arrowLr:
      "M2 12 L8.5 5.5 L8.5 8.8 L15.5 8.8 L15.5 5.5 L22 12 L15.5 18.5 " +
      "L15.5 15.2 L8.5 15.2 L8.5 18.5 Z",
    arrowUd:
      "M12 2 L18.5 8.5 L15.2 8.5 L15.2 15.5 L18.5 15.5 L12 22 L5.5 15.5 " +
      "L8.8 15.5 L8.8 8.5 L5.5 8.5 Z",
    enter: "M3 15 L9.5 9 L9.5 12.6 L14.6 12.6 L14.6 4 L19.4 4 L19.4 17.4 L9.5 17.4 L9.5 21 Z",
    backspace:
      "M8.2 4.5 H21 Q22.5 4.5 22.5 6 V18 Q22.5 19.5 21 19.5 H8.2 L1.5 12 Z " +
      "M14 10.65 L16.75 7.9 L18.1 9.25 L15.35 12 L18.1 14.75 L16.75 16.1 " +
      "L14 13.35 L11.25 16.1 L9.9 14.75 L12.65 12 L9.9 9.25 L11.25 7.9 Z",
  };
  const ROTATE: Partial<Record<KeyIconName, number>> = { right: 90, down: 180, left: 270 };
  const rot = $derived(ROTATE[name]);
</script>

<svg viewBox="0 0 24 24" aria-hidden="true">
  <path
    d={PATHS[name]}
    fill-rule="evenodd"
    transform={rot ? `rotate(${rot} 12 12)` : undefined}
  />
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
