<script lang="ts">
  import { comboToCaps, type Binding, type Platform } from "./keys";

  let {
    bindings,
    platform,
    align = "end",
  }: { bindings: Binding[]; platform: Platform; align?: "start" | "end" } = $props();
</script>

<span class="combos" class:start={align === "start"}>
  {#each bindings as b, bi}
    {#each b.chords as combo, ci}
      <span class="line">
        {#if bi > 0 && ci === 0}<span class="sep">or</span>{/if}
        {#if ci > 0}<span class="sep">then</span>{/if}
        <span class="combo">
          {#each comboToCaps(combo, platform) as cap, j}
            {#if j > 0}<span class="plus">+</span>{/if}
            <kbd class={`k-${b.kind}`} class:underline={cap.underline}>{cap.label || "•"}</kbd>
          {/each}
        </span>
      </span>
    {/each}
  {/each}
</span>

<style>
  /* Alternative bindings stack vertically, right-aligned, so wide entries
     never collide with the command label. */
  .combos {
    display: inline-flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
  }
  .combos.start {
    align-items: flex-start;
  }
  .combo {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    height: 26px;
    padding: 0 7px;
    font-size: 12px;
    font-family: inherit;
    background: var(--cap-bg);
    color: var(--cap-fg);
    border: 1px solid var(--cap-border);
    border-radius: 5px;
    box-shadow: 0 1px 0 var(--cap-border);
  }
  /* Border color says where a binding comes from: default (green) from the
     manifest, custom (yellow) from the user, redefined (mid-gray) = a default
     the user reassigned in the app. */
  kbd.k-default {
    border-color: var(--cap-border-default);
    box-shadow: 0 1px 0 var(--cap-border-default);
  }
  kbd.k-custom {
    border-color: var(--cap-border-custom);
    box-shadow: 0 1px 0 var(--cap-border-custom);
  }
  kbd.k-redefined {
    border-color: var(--cap-border-redefined);
    box-shadow: 0 1px 0 var(--cap-border-redefined);
    color: var(--muted);
  }
  kbd.underline {
    text-decoration: underline;
  }
  .line {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .plus {
    color: var(--muted);
    font-size: 11px;
  }
  .sep {
    color: var(--muted);
    font-size: 11px;
    font-style: italic;
  }
</style>
