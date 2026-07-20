<script lang="ts">
  import type { AssembledEntry, KeyCombo } from "../lib/types";
  import {
    addCustomization,
    onModalEsc,
    removeCustomization,
    setDefaultRedefined,
    setOverlayModal,
  } from "../lib/ipc";
  import { customBindings, defaultBindings, eventToKey, type Platform } from "./keys";
  import KeyVisual from "./KeyVisual.svelte";

  let {
    entry,
    manifestId,
    platform,
    sequence,
    onClose,
  }: {
    entry: AssembledEntry;
    manifestId: string;
    platform: Platform;
    sequence: boolean;
    onClose: () => void;
  } = $props();

  const customs = $derived(customBindings(entry));
  const defaults = $derived(defaultBindings(entry, sequence));

  // --- key capture (VSCode-style: chords accumulate, Enter accepts) ---------
  let capturing = $state(false);
  let captured = $state<KeyCombo[]>([]);
  // The real Win key never reaches the webview (the OS / hook layer owns it),
  // so a Win-based chord is composed with this toggle instead.
  let winMod = $state(false);

  function startCapture() {
    captured = [];
    winMod = false;
    capturing = true;
  }

  function stopCapture() {
    capturing = false;
    captured = [];
    winMod = false;
  }

  async function accept() {
    if (captured.length === 0) return;
    await addCustomization(manifestId, entry.name, $state.snapshot(captured));
    stopCapture();
  }

  function onCaptureKey(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Enter") {
      accept();
      return;
    }
    if (e.key === "Escape") {
      // Browser fallback; under Tauri the agent swallows Esc and it arrives
      // via the modal-esc event instead.
      stopCapture();
      return;
    }
    if (e.key === "Backspace") {
      captured = captured.slice(0, -1);
      return;
    }
    const key = eventToKey(e);
    if (!key) return; // modifier-only press
    captured = [
      ...captured,
      { win: e.metaKey || winMod, ctrl: e.ctrlKey, shift: e.shiftKey, alt: e.altKey, keys: [key] },
    ];
    winMod = false;
  }

  // Capture-phase listener so the overlay's own Esc / Ctrl+F handlers never see
  // keys typed into the capture box.
  $effect(() => {
    if (!capturing) return;
    window.addEventListener("keydown", onCaptureKey, true);
    return () => window.removeEventListener("keydown", onCaptureKey, true);
  });

  // Report the open modal layer so the host routes Esc here before the
  // filter/close logic; reset on unmount.
  $effect(() => {
    setOverlayModal(capturing ? "capture" : "dialog");
  });
  $effect(() => {
    const un = onModalEsc(() => {
      if (capturing) stopCapture();
      else onClose();
    });
    return () => {
      un.then((f) => f());
      setOverlayModal("none");
    };
  });
</script>

<div
  class="scrim"
  onclick={onClose}
  onkeydown={(e) => e.key === "Escape" && onClose()}
  role="presentation"
></div>
<div class="dialog" role="dialog" aria-modal="true" aria-label={`Customize ${entry.name}`}>
  <header>
    <div class="heading">
      <div class="title">{entry.name}</div>
      {#if entry.description}<div class="desc">{entry.description}</div>{/if}
    </div>
    <button class="close" title="Close" onclick={onClose}>✕</button>
  </header>

  <section>
    <h3>Your customizations</h3>
    {#if customs.length === 0 && !capturing}
      <p class="hint">None yet — add one below.</p>
    {/if}
    {#each customs as c (c.raw)}
      <div class="binding">
        <KeyVisual bindings={[c]} {platform} align="start" />
        <button
          class="remove"
          title="Remove this customization"
          onclick={() => removeCustomization(manifestId, entry.name, c.raw ?? "")}
        >
          ✕
        </button>
      </div>
    {/each}

    {#if capturing}
      <div class="capture">
        <div class="capture-keys">
          {#if captured.length === 0}
            <span class="hint">Press the desired key combination…</span>
          {:else}
            <KeyVisual
              bindings={[{ kind: "custom", chords: captured, sequence: captured.length > 1 }]}
              {platform}
              align="start"
            />
          {/if}
          <label
            class="winmod"
            class:active={winMod}
            title="Add the ⊞ Win modifier to the next chord (the physical Win key is reserved by the system)"
          >
            <input type="checkbox" bind:checked={winMod} />
            ⊞
          </label>
        </div>
        <div class="capture-foot">
          <span class="hint">Enter saves · Esc cancels · Backspace removes the last chord</span>
          <span class="capture-actions">
            <button onclick={accept} disabled={captured.length === 0}>Save</button>
            <button onclick={stopCapture}>Cancel</button>
          </span>
        </div>
      </div>
    {:else}
      <button class="add" onclick={startCapture}>＋ Add customization</button>
    {/if}
  </section>

  <section>
    <h3>Defaults</h3>
    {#if defaults.length === 0}
      <p class="hint">This shortcut has no default key binding.</p>
    {/if}
    {#each defaults as d}
      <div class="binding">
        <KeyVisual bindings={[d]} {platform} align="start" />
        <label
          class="redef"
          title="Mark when you have reassigned this default in the app itself, so it no longer works"
        >
          <input
            type="checkbox"
            checked={d.kind === "redefined"}
            onchange={() =>
              setDefaultRedefined(
                manifestId,
                entry.name,
                $state.snapshot(d.chords),
                d.kind !== "redefined",
              )}
          />
          Reassigned
        </label>
      </div>
    {/each}
  </section>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    z-index: 40;
  }
  .dialog {
    position: fixed;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(92vw, 480px);
    max-height: 80vh;
    overflow-y: auto;
    background: var(--bg);
    border: 1px solid var(--divider);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.35);
    padding: 16px 18px;
    z-index: 41;
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin-bottom: 12px;
  }
  .heading {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-size: 15px;
    font-weight: 600;
  }
  .desc {
    font-size: 12px;
    color: var(--muted);
    margin-top: 2px;
  }
  .close {
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font-size: 14px;
    padding: 2px 4px;
  }
  .close:hover {
    color: var(--fg);
  }
  h3 {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    margin: 14px 0 6px;
  }
  .hint {
    font-size: 12px;
    color: var(--muted);
    margin: 4px 0;
  }
  .binding {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 5px 2px;
  }
  .remove {
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font-size: 13px;
    padding: 2px 4px;
  }
  .remove:hover {
    color: var(--fg);
  }
  .redef {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
  }
  .add {
    margin-top: 4px;
    padding: 6px 10px;
    font: inherit;
    font-size: 13px;
    color: var(--fg);
    background: var(--panel);
    border: 1px dashed var(--divider);
    border-radius: 7px;
    cursor: pointer;
  }
  .add:hover {
    border-color: var(--accent);
  }
  .capture {
    margin-top: 6px;
    padding: 10px;
    border: 1px solid var(--accent);
    border-radius: 8px;
    background: var(--panel);
  }
  .capture-keys {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 30px;
  }
  .winmod {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 14px;
    color: var(--muted);
    cursor: pointer;
    white-space: nowrap;
  }
  .winmod.active {
    color: var(--accent);
  }
  .capture-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-top: 8px;
  }
  .capture-actions {
    display: flex;
    gap: 6px;
  }
  .capture-actions button {
    font: inherit;
    font-size: 12px;
    padding: 4px 10px;
    border: 1px solid var(--divider);
    border-radius: 6px;
    background: var(--cap-bg);
    color: var(--fg);
    cursor: pointer;
  }
  .capture-actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
