<script lang="ts">
  // A small "?" badge that reveals a help bubble on hover/focus (click
  // toggles it for touch). Safe to embed inside <label>s: clicks are
  // swallowed so they never activate the labelled control.
  //
  // The bubble is position:fixed and clamped to the viewport, so it never
  // clips at the window edge no matter where the badge sits.
  let { text }: { text: string } = $props();
  let open = $state(false);
  let badge: HTMLElement | undefined = $state();
  let bubbleStyle = $state("");

  const BUBBLE_W = 300;
  const MARGIN = 8;

  function show() {
    if (!badge) return;
    const r = badge.getBoundingClientRect();
    const left = Math.min(
      Math.max(MARGIN, r.left - 10),
      window.innerWidth - BUBBLE_W - MARGIN,
    );
    // Above the badge unless too close to the top, then below.
    bubbleStyle =
      r.top > 200
        ? `left:${left}px; bottom:${window.innerHeight - r.top + MARGIN}px;`
        : `left:${left}px; top:${r.bottom + MARGIN}px;`;
    open = true;
  }

  // A fixed-position bubble would drift from its badge if the form scrolls
  // while open; just close it.
  $effect(() => {
    if (!open) return;
    const close = () => (open = false);
    window.addEventListener("scroll", close, { capture: true, passive: true });
    return () => window.removeEventListener("scroll", close, { capture: true });
  });
</script>

<span class="tipwrap">
  <span
    bind:this={badge}
    class="badge"
    role="button"
    tabindex="0"
    aria-label="Help"
    onmouseenter={show}
    onmouseleave={() => (open = false)}
    onfocus={show}
    onblur={() => (open = false)}
    onclick={(e) => {
      e.preventDefault();
      e.stopPropagation();
      open ? (open = false) : show();
    }}
    onkeydown={(e) => {
      if (e.key === "Escape") open = false;
    }}
  >
    ?
  </span>
  {#if open}
    <span class="bubble" role="tooltip" style={bubbleStyle}>{text}</span>
  {/if}
</span>

<style>
  .tipwrap {
    display: inline-flex;
    vertical-align: middle;
    margin-left: 6px;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    border: 1px solid var(--cap-border);
    background: var(--cap-bg);
    color: var(--muted);
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
    cursor: help;
    user-select: none;
  }
  .badge:hover,
  .badge:focus-visible {
    color: var(--fg);
    border-color: var(--muted);
    outline: none;
  }
  .bubble {
    position: fixed;
    z-index: 20;
    width: 300px;
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--cap-border);
    /* Solid keycap colors: the bubble must stay legible over any text
       behind it, so no translucency here. */
    background: var(--cap-bg);
    color: var(--cap-fg);
    font-size: 12px;
    font-weight: 400;
    line-height: 1.5;
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.25);
    white-space: normal;
  }
</style>
