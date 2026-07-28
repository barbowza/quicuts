<script lang="ts">
  import { invokeCmd, openSettings } from "../lib/ipc";
  import { applyTheme } from "../lib/theme";
  import type { Theme } from "../lib/types";
  import HelpTip from "./HelpTip.svelte";

  interface Chord {
    win: boolean;
    ctrl: boolean;
    shift: boolean;
    alt: boolean;
    vk: number;
  }

  interface Settings {
    schemaVersion: number;
    activation: {
      holdEnabled: boolean;
      holdMs: number;
      chordEnabled: boolean;
      chord: Chord;
      settingsChordEnabled: boolean;
      settingsChord: Chord;
    };
    appearance: {
      theme: Theme;
      overlayStyle: "panel" | "classic";
      panelEdge: "left" | "right";
      panelOpacity: number;
      fontScale: number;
      /** Set by dragging the panel edge, not edited here; must round-trip. */
      panelWidth: number;
      autoWidthResize: boolean;
    };
    excludedExes: string[];
    launchAtLogin: boolean;
    taskbarBadges: boolean;
    autoHide: boolean;
    escClearsFilter: boolean;
    /** Cycled by the panel's footer switch, not edited here; must round-trip. */
    comboDisplayMode: string;
    titleDetection: boolean;
    extraBrowserExes: string[];
  }

  let s = $state<Settings | null>(null);
  let excludedText = $state("");
  let extraBrowserText = $state("");
  let capturing = $state<"chord" | "settingsChord" | null>(null);

  $effect(() => {
    invokeCmd<Settings>("get_settings").then((v) => {
      if (v) {
        s = v;
        excludedText = v.excludedExes.join("\n");
        extraBrowserText = v.extraBrowserExes.join("\n");
        applyTheme(v.appearance.theme);
      }
    });
  });

  async function save() {
    if (!s) return;
    s.excludedExes = excludedText
      .split("\n")
      .map((x) => x.trim())
      .filter(Boolean);
    s.extraBrowserExes = extraBrowserText
      .split("\n")
      .map((x) => x.trim())
      .filter(Boolean);
    applyTheme(s.appearance.theme);
    await invokeCmd("set_settings", { settings: s });
  }

  function onKeydown(e: KeyboardEvent) {
    if (!s) return;
    if (capturing) {
      e.preventDefault();
      // Esc cancels the capture (it would make a hostile chord anyway:
      // Esc closes this window before the chord could toggle it).
      if (e.key === "Escape") {
        capturing = null;
        return;
      }
      const mod = e.key === "Control" || e.key === "Shift" || e.key === "Alt" || e.key === "Meta";
      if (mod) return;
      s.activation[capturing] = {
        win: e.metaKey,
        ctrl: e.ctrlKey,
        shift: e.shiftKey,
        alt: e.altKey,
        vk: e.keyCode,
      };
      capturing = null;
      save();
      return;
    }
    // Esc closes the window (open_settings toggles: we're visible+focused).
    if (e.key === "Escape") {
      e.preventDefault();
      openSettings();
      return;
    }
    // The settings chord also closes this window (open_settings toggles);
    // without this, the chord only works while the panel has focus.
    const c = s.activation.settingsChord;
    if (
      s.activation.settingsChordEnabled &&
      e.keyCode === c.vk &&
      e.ctrlKey === c.ctrl &&
      e.shiftKey === c.shift &&
      e.altKey === c.alt &&
      e.metaKey === c.win
    ) {
      e.preventDefault();
      openSettings();
    }
  }

  // Map a Windows virtual-key code to its keycap label, when known.
  function vkName(vk: number): string | null {
    if (vk >= 0x41 && vk <= 0x5a) return String.fromCharCode(vk); // A-Z
    if (vk >= 0x30 && vk <= 0x39) return String.fromCharCode(vk); // 0-9
    if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`; // F1-F24
    const named: Record<number, string> = {
      0x08: "Backspace", 0x09: "Tab", 0x0d: "Enter", 0x1b: "Esc", 0x20: "Space",
      0x21: "Page Up", 0x22: "Page Down", 0x23: "End", 0x24: "Home",
      0x25: "←", 0x26: "↑", 0x27: "→", 0x28: "↓",
      0x2d: "Insert", 0x2e: "Delete",
      0xba: ";", 0xbb: "=", 0xbc: ",", 0xbd: "-", 0xbe: ".", 0xbf: "/",
      0xc0: "`", 0xdb: "[", 0xdc: "\\", 0xdd: "]", 0xde: "'",
    };
    return named[vk] ?? null;
  }

  function chordLabel(c: Chord): string {
    const parts: string[] = [];
    if (c.win) parts.push("Win");
    if (c.ctrl) parts.push("Ctrl");
    if (c.alt) parts.push("Alt");
    if (c.shift) parts.push("Shift");
    const hex = `0x${c.vk.toString(16).toUpperCase()}`;
    const name = vkName(c.vk);
    parts.push(name ? `${name} (${hex})` : hex);
    return parts.join(" + ");
  }
</script>

<svelte:window on:keydown={onKeydown} />

<div class="wrap">
  <h1>Quicuts settings</h1>
  {#if !s}
    <p>Loading…</p>
  {:else}
    <section>
      <h2>Activation</h2>
      <label class="check">
        <input type="checkbox" bind:checked={s.activation.holdEnabled} onchange={save} />
        Hold the Windows / Command key to show
      </label>
      <label class="range">
        Hold duration: {s.activation.holdMs} ms
        <input
          type="range"
          min="200"
          max="2000"
          step="50"
          bind:value={s.activation.holdMs}
          onchange={save}
          disabled={!s.activation.holdEnabled}
        />
      </label>
      <label class="col">
        <span>
          Excluded apps (one exe per line)
          <HelpTip
            text="While one of these apps is in front, holding the Windows key won't open the panel — handy for games, remote desktops, or virtual machines that need the key for themselves. Type each app's program name, e.g. game.exe, one per line. The activation chord still works everywhere."
          />
        </span>
        <textarea rows="2" bind:value={excludedText} onblur={save}></textarea>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={s.activation.chordEnabled} onchange={save} />
        Show with a hotkey
      </label>
      <div class="chord">
        <code>{chordLabel(s.activation.chord)}</code>
        <button onclick={() => (capturing = "chord")} disabled={!s.activation.chordEnabled}>
          {capturing === "chord" ? "Press keys…" : "Change"}
        </button>
      </div>
      <label class="check">
        <input type="checkbox" bind:checked={s.activation.settingsChordEnabled} onchange={save} />
        Open / close settings with a hotkey (while the panel is focused)
      </label>
      <div class="chord">
        <code>{chordLabel(s.activation.settingsChord)}</code>
        <button
          onclick={() => (capturing = "settingsChord")}
          disabled={!s.activation.settingsChordEnabled}
        >
          {capturing === "settingsChord" ? "Press keys…" : "Change"}
        </button>
      </div>
    </section>

    <section>
      <h2>Appearance</h2>
      <label>
        Theme
        <select bind:value={s.appearance.theme} onchange={save}>
          <option value="system">System</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </label>
      <label>
        Panel edge
        <select bind:value={s.appearance.panelEdge} onchange={save}>
          <option value="right">Right</option>
          <option value="left">Left</option>
        </select>
      </label>
      <label class="range">
        Panel opacity: {Math.round(s.appearance.panelOpacity * 100)}%
        <input
          type="range"
          min="0"
          max="100"
          step="5"
          value={Math.round(s.appearance.panelOpacity * 100)}
          oninput={(e) => {
            if (s) s.appearance.panelOpacity = e.currentTarget.valueAsNumber / 100;
          }}
          onchange={save}
        />
      </label>
      <label class="range">
        Font size: {Math.round(s.appearance.fontScale * 100)}%
        <input
          type="range"
          min="80"
          max="200"
          step="5"
          value={Math.round(s.appearance.fontScale * 100)}
          oninput={(e) => {
            if (s) s.appearance.fontScale = e.currentTarget.valueAsNumber / 100;
          }}
          onchange={save}
        />
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={s.appearance.autoWidthResize} onchange={save} />
        Resize panel with font size
        <HelpTip
          text="The panel's width can always be adjusted by dragging its inner edge, like a normal window. With this on, changing the font size also widens or narrows the panel by the same amount; with it off, the text reflows at the current width. Ctrl+Plus, Ctrl+Minus, and Ctrl+0 change the font size while the panel is focused."
        />
      </label>
    </section>

    <section>
      <h2>Behavior</h2>
      <label class="check">
        <input type="checkbox" bind:checked={s.autoHide} onchange={save} />
        Auto-hide the panel when it loses focus
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={s.escClearsFilter} onchange={save} />
        Esc clears the filter first; a second Esc closes the panel
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={s.launchAtLogin} onchange={save} />
        Launch at login
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={s.taskbarBadges} onchange={save} />
        Show taskbar number badges (Windows)
      </label>
    </section>

    <section>
      <h2>Web apps (experimental)</h2>
      <label class="check">
        <input type="checkbox" bind:checked={s.titleDetection} onchange={save} />
        Detect web apps by window title (experimental)
        <HelpTip
          text="When you're working in a web app Quicuts knows — like Gmail — the panel switches to that app's shortcuts automatically by watching the window's title. Experimental: site titles vary, so detection can occasionally miss."
        />
      </label>
      <label class="col">
        <span>
          Additional browsers (one exe per line)
          <HelpTip
            text="Quicuts already recognizes the major browsers — Chrome, Edge, Firefox, Brave, Opera, Vivaldi, and more. Only add yours here if web apps like Gmail don't show up in it: type the browser's program name, e.g. mybrowser.exe, one per line."
          />
        </span>
        <textarea rows="2" bind:value={extraBrowserText} onblur={save}></textarea>
      </label>
    </section>
  {/if}
</div>

<style>
  .wrap {
    padding: 20px 26px;
    height: 100vh;
    overflow-y: auto;
    background: var(--bg);
  }
  /* Cap the form content, not the window: extra width shows themed
     background instead of the unstyled (white) page behind the wrap. */
  .wrap > :global(*) {
    max-width: 560px;
  }
  h1 {
    font-size: 18px;
    margin: 0 0 16px;
  }
  section {
    margin-bottom: 22px;
  }
  h2 {
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    margin-bottom: 8px;
  }
  label {
    display: block;
    font-size: 13px;
    margin-bottom: 10px;
  }
  label.check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  label.range input {
    display: block;
    width: 100%;
    margin-top: 4px;
  }
  label.col textarea {
    display: block;
    width: 100%;
    margin-top: 4px;
    font-family: inherit;
  }
  .chord {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  code {
    background: var(--cap-bg);
    border: 1px solid var(--cap-border);
    padding: 3px 8px;
    border-radius: 5px;
  }
  select,
  button {
    font-family: inherit;
    font-size: 13px;
  }
</style>
