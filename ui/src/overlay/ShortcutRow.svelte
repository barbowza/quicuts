<script lang="ts">
  import type { AssembledEntry, ComboDisplayMode } from "../lib/types";
  import { entryBindings, type Platform } from "./keys";
  import KeyVisual from "./KeyVisual.svelte";
  import { togglePin } from "../lib/ipc";

  let {
    entry,
    manifestId,
    platform,
    sequence = false,
    mode,
    onCustomize,
  }: {
    entry: AssembledEntry;
    manifestId: string;
    platform: Platform;
    sequence?: boolean;
    mode: ComboDisplayMode;
    onCustomize: (entry: AssembledEntry) => void;
  } = $props();

  const bindings = $derived(entryBindings(entry, sequence, mode));

  function maybeCustomize(e: MouseEvent) {
    // A double-click on the pin star must not also open the dialog.
    if (e.target instanceof Element && e.target.closest("button")) return;
    onCustomize(entry);
  }
</script>

<div
  class="row"
  role="button"
  tabindex="-1"
  title="Double-click to customize"
  ondblclick={maybeCustomize}
  onkeydown={(e) => e.key === "Enter" && onCustomize(entry)}
>
  <button
    class="pin"
    class:active={entry.pinned}
    title={entry.pinned ? "Unpin" : "Pin"}
    onclick={() => togglePin(manifestId, entry.key)}
  >
    {entry.pinned ? "★" : "☆"}
  </button>
  <div class="text">
    <div class="name">{entry.name}</div>
    {#if entry.description}<div class="desc">{entry.description}</div>{/if}
  </div>
  <div class="keys">
    <KeyVisual {bindings} {platform} />
  </div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 8px;
    border-radius: 6px;
  }
  .row:hover {
    background: var(--rail-hover);
  }
  .pin {
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font-size: 0.875rem;
    line-height: 1;
    padding: 2px;
  }
  .pin.active {
    color: var(--accent);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-size: 0.8125rem;
  }
  .desc {
    font-size: 0.6875rem;
    color: var(--muted);
  }
  .keys {
    flex-shrink: 0;
  }
</style>
