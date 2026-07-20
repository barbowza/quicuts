<script lang="ts">
  import type { AssembledEntry, AssembledSection, ComboDisplayMode } from "../lib/types";
  import type { Platform } from "./keys";
  import ShortcutRow from "./ShortcutRow.svelte";

  let {
    section,
    manifestId,
    platform,
    seqKeys,
    mode,
    onCustomize,
  }: {
    section: AssembledSection;
    manifestId: string;
    platform: Platform;
    seqKeys: Set<string>;
    mode: ComboDisplayMode;
    onCustomize: (entry: AssembledEntry) => void;
  } = $props();
</script>

<section>
  <h2 class:taskbar={section.kind === "taskbar"}>{section.title}</h2>
  {#if section.entries.length === 0}
    <p class="empty">
      {section.kind === "pinned" ? "Pin shortcuts with the star to keep them here." : "Nothing here."}
    </p>
  {:else}
    {#each section.entries as entry (entry.key)}
      <ShortcutRow
        {entry}
        {manifestId}
        {platform}
        sequence={seqKeys.has(entry.key)}
        {mode}
        {onCustomize}
      />
    {/each}
  {/if}
</section>

<style>
  section {
    margin-bottom: 18px;
  }
  h2 {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    margin: 0 0 4px 8px;
  }
  h2.taskbar::after {
    content: " ⌗";
    color: var(--accent);
  }
  .empty {
    font-size: 12px;
    color: var(--muted);
    margin: 2px 8px;
  }
</style>
