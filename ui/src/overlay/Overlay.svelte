<script lang="ts">
  import { tick } from "svelte";
  import type { AssembledEntry, ComboDisplayMode, OverlayState } from "../lib/types";
  import AppRail from "./AppRail.svelte";
  import CustomizeDialog from "./CustomizeDialog.svelte";
  import Help from "./Help.svelte";
  import Section from "./Section.svelte";
  import {
    onOverlayState,
    onAppearance,
    onClearFilter,
    getAppearance,
    dismissOverlay,
    setComboDisplayMode,
    setFilterActive,
    selectApp,
    setPinnedApp,
    openSettings,
    adjustFontScale,
  } from "../lib/ipc";
  import { applyAppearance } from "../lib/theme";
  import { comboToCaps, sequenceEntryKeys } from "./keys";

  let view = $state<OverlayState>({
    platform: "windows",
    apps: [],
    pages: {},
    selected: null,
    pinnedApp: null,
    comboDisplayMode: "all",
    holdEnabled: false,
    chordEnabled: false,
    chord: { win: true, ctrl: false, shift: true, alt: false, vk: 0xbf },
    settingsChordEnabled: false,
    settingsChord: { win: false, ctrl: true, shift: false, alt: false, vk: 0xbc },
  });

  const sections = $derived(view.selected ? (view.pages[view.selected] ?? []) : []);
  // Unsupported-app placeholder (ADR 0004): its page is a bare message, no
  // filter box (there is nothing to filter).
  const selectedApp = $derived(view.apps.find((a) => a.manifestId === view.selected));
  const unsupported = $derived(selectedApp?.unsupported ?? false);
  // Sequence detection runs on the full page so prefix counting isn't
  // affected by the filter.
  const seqKeys = $derived(sequenceEntryKeys(sections));
  const mode = $derived<ComboDisplayMode>(view.comboDisplayMode ?? "all");

  // Filter box: Ctrl+F focuses it; matches name, description, and key labels.
  let filter = $state("");
  let filterInput = $state<HTMLInputElement | null>(null);

  // Help view (Ctrl+H / the footer Help button) replaces the shortcut list.
  let helpOpen = $state(false);
  function toggleHelp() {
    helpOpen = !helpOpen;
    // Help hides the rows the customize dialog anchors to.
    if (helpOpen) customizeKey = null;
  }

  function entryText(e: AssembledEntry): string {
    const chords = [...e.combos, ...(e.customCombos ?? []).flatMap((c) => c.chords)];
    // Glyph caps carry a typeable alias ("⊞" -> "win"). It sits between the
    // glyph and the "+" join, so both "win" and chord-style "win+e" match.
    const keys = chords
      .map((c) =>
        comboToCaps(c, view.platform)
          .map((cap) => (cap.search ? `${cap.label} ${cap.search}` : cap.label))
          .join("+"),
      )
      .join(" ");
    return `${e.name} ${e.description ?? ""} ${keys}`.toLowerCase();
  }

  const filteredSections = $derived.by(() => {
    // Whitespace splits the query into AND-ed terms ("ctrl space" = entries
    // matching both, in the keys or the text); "win+e" stays one term and
    // matches the chord text exactly.
    const terms = filter.trim().toLowerCase().split(/\s+/).filter(Boolean);
    let secs = sections;
    if (terms.length > 0) {
      secs = secs
        .map((s) => ({
          ...s,
          entries: s.entries.filter((e) => {
            const text = entryText(e);
            return terms.every((t) => text.includes(t));
          }),
        }))
        .filter((s) => s.entries.length > 0);
    }
    // Custom-only mode reads as "show me my customizations": entries (and
    // sections) without any drop out entirely.
    if (mode === "custom") {
      secs = secs
        .map((s) => ({
          ...s,
          entries: s.entries.filter((e) => (e.customCombos ?? []).length > 0),
        }))
        .filter((s) => s.entries.length > 0);
    }
    return secs;
  });

  // --- shortcut customization dialog (opened by double-clicking a row) ------
  let customizeKey = $state<string | null>(null);
  const customizeEntry = $derived.by(() => {
    if (!customizeKey) return null;
    for (const s of sections) {
      const hit = s.entries.find((e) => e.key === customizeKey);
      if (hit) return hit;
    }
    return null;
  });

  // --- four-way combo display switch (footer) --------------------------------
  const MODES: { id: ComboDisplayMode; label: string }[] = [
    { id: "default", label: "Defaults" },
    { id: "custom", label: "Custom" },
    { id: "all", label: "All" },
    { id: "customElseDefault", label: "Custom ▸ defaults" },
  ];
  const modeLabel = $derived(MODES.find((m) => m.id === mode)?.label ?? "All");
  function cycleMode() {
    const i = MODES.findIndex((m) => m.id === mode);
    const next = MODES[(i + 1) % MODES.length].id;
    view.comboDisplayMode = next; // optimistic; the host re-pushes state
    setComboDisplayMode(next);
  }

  // Apps box: collapsed shows only the selected app; snaps shut on switch.
  let appsCollapsed = $state(true);

  // A stale filter from the previous app would silently hide shortcuts.
  let prevSelected: string | null = null;
  $effect(() => {
    if (view.selected !== prevSelected) {
      prevSelected = view.selected;
      filter = "";
      appsCollapsed = true;
      customizeKey = null;
      helpOpen = false;
    }
  });

  // The customized entry can vanish under the dialog (manifest reload,
  // custom-only filtering elsewhere); close rather than showing a stale one.
  $effect(() => {
    if (customizeKey && !customizeEntry) customizeKey = null;
  });

  // Report empty<->non-empty transitions to the host, which owns the Esc
  // decision (clear filter vs close) because the agent swallows Esc while
  // the panel is focused.
  let prevFilterActive = false;
  $effect(() => {
    const active = filter.trim().length > 0;
    if (active !== prevFilterActive) {
      prevFilterActive = active;
      setFilterActive(active);
    }
  });

  // Focus tell: accent border while the panel window is focused (the sticky
  // panel stays visible unfocused, so the user needs to see which is which).
  let focused = $state(false);

  $effect(() => {
    const unlisten = onOverlayState((s) => (view = s));
    const unlistenAppearance = onAppearance((a) => applyAppearance(a, true));
    const unlistenClear = onClearFilter(() => (filter = ""));
    getAppearance().then((a) => a && applyAppearance(a, true));
    const onKey = (e: KeyboardEvent) => {
      // Settings chord (default Ctrl+,): only while the panel has focus —
      // keydown never reaches this webview otherwise. The host toggles.
      const c = view.settingsChord;
      if (
        view.settingsChordEnabled &&
        e.keyCode === c.vk &&
        e.ctrlKey === c.ctrl &&
        e.shiftKey === c.shift &&
        e.altKey === c.alt &&
        e.metaKey === c.win
      ) {
        e.preventDefault();
        openSettings();
        return;
      }
      // Rarely reached under Tauri (the agent swallows Esc and routes it via
      // Dismissed); same two-step logic host-side either way.
      if (e.key === "Escape") dismissOverlay();
      // Take over Ctrl+F from WebView2's native find bar (whose window would
      // otherwise even count as a foreground change).
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        // The input isn't mounted while help is up; focus after the swap.
        helpOpen = false;
        tick().then(() => {
          filterInput?.focus();
          filterInput?.select();
        });
      }
      // Ctrl+H toggles the help view (WebView2's history shortcut is unused
      // here). Capture-phase listeners (key capture) never let this fire.
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "h") {
        e.preventDefault();
        toggleHelp();
      }
      // Font scale (ADR 0005): Ctrl+=/+ up, Ctrl+- down, Ctrl+0 reset.
      // preventDefault keeps WebView2's own browser zoom out of the way.
      // e.key covers the numpad +/- too; Shift stays allowed ("+" is
      // Shift+= on most layouts).
      if ((e.ctrlKey || e.metaKey) && !e.altKey) {
        if (e.key === "=" || e.key === "+") {
          e.preventDefault();
          adjustFontScale("increase");
        } else if (e.key === "-") {
          e.preventDefault();
          adjustFontScale("decrease");
        } else if (e.key === "0") {
          e.preventDefault();
          adjustFontScale("reset");
        }
      }
    };
    const onFocus = () => (focused = true);
    const onBlur = () => (focused = false);
    window.addEventListener("keydown", onKey);
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    return () => {
      unlisten.then((f) => f());
      unlistenAppearance.then((f) => f());
      unlistenClear.then((f) => f());
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  });

  function pick(id: string) {
    view.selected = id;
    appsCollapsed = true;
    selectApp(id);
  }
</script>

<div class="panel" class:focused>
  <AppRail
    apps={view.apps}
    selected={view.selected}
    pinnedApp={view.pinnedApp}
    collapsed={appsCollapsed}
    onSelect={pick}
    onTogglePinApp={(id) => setPinnedApp(id)}
    onToggleCollapsed={() => (appsCollapsed = !appsCollapsed)}
  />
  <main class="content">
    {#if helpOpen}
      <Help
        platform={view.platform}
        holdEnabled={view.holdEnabled}
        chordEnabled={view.chordEnabled}
        chord={view.chord}
        settingsChordEnabled={view.settingsChordEnabled}
        settingsChord={view.settingsChord}
        onClose={() => (helpOpen = false)}
      />
    {:else if unsupported}
      <div class="unsupported">No shortcuts for {selectedApp?.displayName}.</div>
    {:else}
      <input
        class="filter"
        type="text"
        placeholder="Filter shortcuts (Ctrl+F)"
        spellcheck="false"
        bind:value={filter}
        bind:this={filterInput}
      />
      {#if filteredSections.length === 0}
        <div class="placeholder">
          {filter.trim()
            ? `No shortcuts match “${filter.trim()}”.`
            : mode === "custom"
              ? "No customized shortcuts for this app yet. Double-click a shortcut to add one."
              : "No shortcuts for this app yet."}
        </div>
      {:else}
        {#each filteredSections as section (section.kind + section.title)}
          <Section
            {section}
            manifestId={view.selected ?? ""}
            platform={view.platform}
            {seqKeys}
            {mode}
            onCustomize={(entry) => (customizeKey = entry.key)}
          />
        {/each}
      {/if}
    {/if}
  </main>
  <footer class="buttons">
    <button class="gear" title="Settings" onclick={() => openSettings()}>
      <span aria-hidden="true">⚙</span>
      <span>Settings</span>
    </button>
    <button
      class="gear modeswitch"
      title="Which key combos to show — click to cycle (defaults → custom → all → custom, else defaults)"
      onclick={cycleMode}
    >
      <span aria-hidden="true">⌨</span>
      <span>{modeLabel}</span>
    </button>
    <button class="gear" title="Help (Ctrl+H)" onclick={toggleHelp}>
      <svg
        aria-hidden="true"
        style="width: 0.9375rem; height: 0.9375rem"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
      >
        <circle cx="12" cy="12" r="10" />
        <circle cx="12" cy="12" r="4" />
        <line x1="4.93" y1="4.93" x2="9.17" y2="9.17" />
        <line x1="14.83" y1="14.83" x2="19.07" y2="19.07" />
        <line x1="14.83" y1="9.17" x2="19.07" y2="4.93" />
        <line x1="4.93" y1="19.07" x2="9.17" y2="14.83" />
      </svg>
      <span>Help</span>
    </button>
  </footer>
  {#if customizeEntry && view.selected}
    <CustomizeDialog
      entry={customizeEntry}
      manifestId={view.selected}
      platform={view.platform}
      sequence={seqKeys.has(customizeEntry.key)}
      onClose={() => (customizeKey = null)}
    />
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg);
  }
  .panel.focused {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    border-top: 1px solid var(--divider);
    padding: 5px 10px;
  }
  .gear {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 10px;
    border: none;
    background: transparent;
    color: var(--fg);
    border-radius: 7px;
    cursor: pointer;
    font-size: 0.8125rem;
  }
  .gear:hover {
    background: var(--rail-hover);
  }
  .gear span[aria-hidden] {
    font-size: 0.9375rem;
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 14px 16px;
  }
  .filter {
    width: 100%;
    padding: 7px 10px;
    margin-bottom: 12px;
    font: inherit;
    font-size: 0.8125rem;
    color: var(--fg);
    background: var(--panel);
    border: 1px solid var(--divider);
    border-radius: 7px;
    outline: none;
  }
  .filter:focus {
    border-color: var(--accent);
  }
  .filter::placeholder {
    color: var(--muted);
  }
  .placeholder {
    color: var(--muted);
    font-size: 0.8125rem;
    padding: 20px 8px;
  }
  .unsupported {
    color: var(--muted);
    font-size: 0.8125rem;
    text-align: center;
    padding: 48px 16px;
  }
</style>
