<script lang="ts">
  import type { RailApp } from "../lib/types";

  let {
    apps,
    selected,
    pinnedApp,
    collapsed,
    onSelect,
    onTogglePinApp,
    onToggleCollapsed,
  }: {
    apps: RailApp[];
    selected: string | null;
    pinnedApp: string | null;
    collapsed: boolean;
    onSelect: (id: string) => void;
    onTogglePinApp: (id: string | null) => void;
    onToggleCollapsed: () => void;
  } = $props();

  const current = $derived(apps.find((a) => a.manifestId === selected) ?? apps[0]);
  const shown = $derived(collapsed ? (current ? [current] : []) : apps);
</script>

<nav class="rail" class:expanded={!collapsed}>
  {#each shown as app (app.manifestId)}
    <div class="row" class:selected={app.manifestId === selected}>
      <button
        class="item"
        class:unsupported={app.unsupported}
        onclick={() => (collapsed ? onToggleCollapsed() : onSelect(app.manifestId))}
        title={collapsed ? "Show all apps" : app.displayName}
      >
        {#if app.iconUrl}
          <img src={app.iconUrl} alt="" />
        {:else}
          <span class="tile">{app.displayName.charAt(0).toUpperCase() || "?"}</span>
        {/if}
        <span class="label">{app.displayName}</span>
      </button>
      {#if !app.unsupported}
        <button
          class="pin"
          class:active={app.manifestId === pinnedApp}
          title={app.manifestId === pinnedApp ? "Unpin" : "Pin this app"}
          onclick={() => onTogglePinApp(app.manifestId === pinnedApp ? null : app.manifestId)}
        >
          📌
        </button>
      {/if}
    </div>
  {/each}
  {#if apps.length > 1}
    <button
      class="chevron"
      title={collapsed ? "Show all apps" : "Collapse"}
      onclick={onToggleCollapsed}
    >
      {collapsed ? "▾" : "▴"}
    </button>
  {/if}
</nav>

<style>
  .rail {
    flex-shrink: 0;
    padding: 8px 10px 0;
    border-bottom: 1px solid var(--divider);
  }
  .rail.expanded {
    max-height: 40vh;
    overflow-y: auto;
  }
  .row {
    display: flex;
    align-items: center;
    border-radius: 7px;
  }
  .row:hover {
    background: var(--rail-hover);
  }
  .row.selected {
    background: var(--rail-selected);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    padding: 7px 8px;
    border: none;
    background: transparent;
    color: var(--fg);
    border-radius: 7px;
    cursor: pointer;
    text-align: left;
  }
  /* Unsupported-app placeholder: dimmed to signal "no shortcuts here". */
  .item.unsupported {
    opacity: 0.55;
  }
  .item.unsupported .label {
    color: var(--muted);
  }
  .pin {
    flex-shrink: 0;
    border: none;
    background: transparent;
    padding: 4px 6px;
    cursor: pointer;
    font-size: 0.6875rem;
    border-radius: 5px;
    /* Hidden until the row is hovered or the pin is set. */
    opacity: 0;
    filter: grayscale(1);
  }
  .row:hover .pin {
    opacity: 0.6;
  }
  .pin:hover {
    opacity: 1 !important;
  }
  .pin.active {
    opacity: 1;
    filter: none;
  }
  .chevron {
    display: block;
    width: 100%;
    padding: 0 0 3px;
    border: none;
    background: transparent;
    color: var(--muted);
    font-size: 0.6875rem;
    line-height: 1.4;
    cursor: pointer;
  }
  .chevron:hover {
    color: var(--fg);
  }
  img {
    width: 1.125rem;
    height: 1.125rem;
    flex-shrink: 0;
  }
  /* Letter-tile fallback for apps without an icon (e.g. hosted
     collections that ship no Icon file). */
  .tile {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1.125rem;
    height: 1.125rem;
    flex-shrink: 0;
    border-radius: 4px;
    background: var(--cap-bg);
    border: 1px solid var(--cap-border);
    color: var(--cap-fg);
    font-size: 0.6875rem;
    font-weight: 600;
    line-height: 1;
  }
  .label {
    font-size: 0.78125rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
