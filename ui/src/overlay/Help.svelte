<script lang="ts">
  import type { ChordSpec, KeyCombo } from "../lib/types";
  import { onModalEsc, setOverlayModal } from "../lib/ipc";
  import { comboToCaps, type Binding, type Platform } from "./keys";
  import KeyVisual from "./KeyVisual.svelte";

  let {
    platform,
    holdEnabled,
    chordEnabled,
    chord,
    settingsChordEnabled,
    settingsChord,
    onClose,
  }: {
    platform: Platform;
    holdEnabled: boolean;
    chordEnabled: boolean;
    chord: ChordSpec;
    settingsChordEnabled: boolean;
    settingsChord: ChordSpec;
    onClose: () => void;
  } = $props();

  function chordCombo(c: ChordSpec): KeyCombo {
    return { win: c.win, ctrl: c.ctrl, shift: c.shift, alt: c.alt, keys: [{ kind: "vk", value: c.vk }] };
  }
  function asBindings(combo: KeyCombo): Binding[] {
    return [{ kind: "default", chords: [combo], sequence: false }];
  }
  const ctrlKey = (letter: string): KeyCombo => ({
    win: false,
    ctrl: true,
    shift: false,
    alt: false,
    keys: [{ kind: "literal", value: letter }],
  });

  // Quicuts' own shortcuts, rendered like a regular shortcut section. The
  // chord rows track the configured chords and hide when disabled.
  const quicutsRows = $derived<{ name: string; desc: string; bindings: Binding[] }[]>([
    ...(holdEnabled
      ? [
          {
            name: "Peek the panel",
            desc: "Hold to show shortcuts for the current app, release to hide",
            bindings: asBindings({ win: true, ctrl: false, shift: false, alt: false, keys: [] }),
          },
        ]
      : []),
    ...(chordEnabled
      ? [
          {
            name: "Toggle the panel",
            desc: "Keeps the panel on screen until pressed again",
            bindings: asBindings(chordCombo(chord)),
          },
        ]
      : []),
    { name: "Find shortcuts", desc: "Focus the find box", bindings: asBindings(ctrlKey("F")) },
    { name: "Help", desc: "Toggle this page", bindings: asBindings(ctrlKey("H")) },
    ...(settingsChordEnabled
      ? [
          {
            name: "Open settings",
            desc: "While the panel is focused",
            bindings: asBindings(chordCombo(settingsChord)),
          },
        ]
      : []),
    {
      name: "Close",
      desc: "Dismiss this help, a dialog, or the panel",
      bindings: asBindings({
        win: false,
        ctrl: false,
        shift: false,
        alt: false,
        keys: [{ kind: "glyph", value: "escape" }],
      }),
    },
  ]);

  // The *configured* settings chord, rendered with the same keycaps as the
  // shortcut lists (it may not be the Ctrl+, default).
  const settingsCaps = $derived(
    comboToCaps(
      {
        win: settingsChord.win,
        ctrl: settingsChord.ctrl,
        shift: settingsChord.shift,
        alt: settingsChord.alt,
        keys: [{ kind: "vk", value: settingsChord.vk }],
      } satisfies KeyCombo,
      platform,
    ),
  );

  // Same Esc routing as CustomizeDialog: report the open modal layer so the
  // host sends the agent-swallowed Esc here (close help) before panel-close.
  $effect(() => {
    setOverlayModal("dialog");
    const un = onModalEsc(onClose);
    return () => {
      un.then((f) => f());
      setOverlayModal("none");
    };
  });
</script>

<div class="help">
  <header>
    <div class="title">Quicuts help</div>
    <button class="close" title="Close (Esc)" onclick={onClose}>✕</button>
  </header>

  <section>
    <h3>Quicuts shortcuts</h3>
    <div class="qshortcuts">
      {#each quicutsRows as row (row.name)}
        <div class="row">
          <div class="text">
            <div class="name">{row.name}</div>
            {#if row.desc}<div class="desc">{row.desc}</div>{/if}
          </div>
          <div class="keys">
            <KeyVisual bindings={row.bindings} {platform} />
          </div>
        </div>
      {/each}
    </div>
  </section>

  <section>
    <h3>Showing the panel</h3>
    <p>
      Hold the <kbd>⊞</kbd> key to peek at the shortcuts for the app you're using — release to
      hide. Press the activation chord (default <kbd>⊞</kbd><kbd>⇧</kbd><kbd>/</kbd>, changeable
      in Settings) to keep the panel on screen; press it again or <kbd>Esc</kbd> to close it.
    </p>
    <p>
      Real shortcuts still work while the panel is up: pressing something like
      <kbd>⊞</kbd><kbd>E</kbd> passes through to Windows and hides the panel.
    </p>
  </section>

  <section>
    <h3>Switching apps</h3>
    <p>
      The left rail lists apps that have shortcut pages; click one to view its shortcuts. The 📌
      on a rail entry pins that app so the panel always opens on it.
    </p>
  </section>

  <section>
    <h3>Finding shortcuts (Ctrl+F)</h3>
    <p><kbd>Ctrl</kbd><kbd>F</kbd> focuses the find box. Matching rules:</p>
    <ul>
      <li>
        Separate words with spaces to require all of them — <code>ctrl space</code> finds every
        shortcut whose keys or description contain both.
      </li>
      <li>
        Glyph keys are typeable by name: <code>win</code>, <code>shift</code>, <code>alt</code>,
        <code>enter</code>, <code>backspace</code>, <code>left</code>/<code>right</code>/<code
        >up</code>/<code>down</code>…
      </li>
      <li>Words also match shortcut names and descriptions (<code>ctrl paste</code>).</li>
      <li><code>win+e</code> (with a plus) matches that exact chord.</li>
    </ul>
    <p>
      <kbd>Esc</kbd> clears the find box first, then closes the panel (when the two-step option
      is enabled in Settings).
    </p>
  </section>

  <section>
    <h3>Reading the keycaps</h3>
    <p>The keycap border says where a binding comes from:</p>
    <ul class="legend">
      <li><kbd class="k-default">A</kbd> app default, from the shortcut manifest</li>
      <li><kbd class="k-custom">A</kbd> your own custom combo</li>
      <li><kbd class="k-redefined">A</kbd> a default you marked as reassigned</li>
    </ul>
    <p>
      Combos joined by <em>then</em> are pressed in sequence (e.g. <kbd>Ctrl</kbd><kbd>K</kbd>
      then <kbd>Ctrl</kbd><kbd>S</kbd>); combos joined by <em>or</em> are alternatives.
    </p>
  </section>

  <section>
    <h3>Customizing shortcuts</h3>
    <p>
      Double-click a shortcut row to open its customization dialog: add your own key combos
      (type the chords, <kbd>↵</kbd> saves) or mark a default as reassigned when you've changed
      it in the app itself.
    </p>
  </section>

  <section>
    <h3>Display modes</h3>
    <p>
      The ⌨ button in the footer cycles which combos are shown: Defaults → Custom → All →
      Custom&nbsp;▸&nbsp;defaults (customs where you have them, defaults otherwise).
    </p>
  </section>

  <section>
    <h3>Taskbar badges</h3>
    <p>
      While the panel is up, numbered chips appear over the taskbar buttons showing what
      <kbd>⊞</kbd><kbd>1</kbd>…<kbd>9</kbd> launches. Toggle them in Settings.
    </p>
  </section>

  <section>
    <h3>Settings</h3>
    <p>
      Open with the ⚙ button{#if settingsChordEnabled}
        or press
        {#each settingsCaps as cap, i}{#if i > 0}<span class="plus">+</span>{/if}<kbd
          >{cap.label}</kbd
        >{/each}
        while the panel is focused{/if}. Activation, appearance, panel edge, and the options
      mentioned above all live there.
    </p>
  </section>

  <section>
    <h3>This help</h3>
    <p><kbd>Ctrl</kbd><kbd>H</kbd> or the ? button toggles this page; <kbd>Esc</kbd> closes it.</p>
  </section>
</div>

<style>
  .help {
    font-size: 13px;
    line-height: 1.55;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 4px;
  }
  .title {
    flex: 1;
    font-size: 15px;
    font-weight: 600;
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
  /* Row chrome copied from ShortcutRow.svelte (minus the pin/customize
     affordances) so this panel reads like the regular shortcut list. */
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 8px;
    border-radius: 6px;
    line-height: normal;
  }
  .row:hover {
    background: var(--rail-hover);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-size: 13px;
  }
  .desc {
    font-size: 11px;
    color: var(--muted);
  }
  .keys {
    flex-shrink: 0;
  }
  h3 {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
    margin: 16px 0 4px;
  }
  p {
    margin: 4px 0;
  }
  ul {
    margin: 4px 0;
    padding-left: 18px;
  }
  li {
    margin: 2px 0;
  }
  .legend {
    list-style: none;
    padding-left: 2px;
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 0;
  }
  code {
    padding: 1px 5px;
    font-size: 12px;
    background: var(--panel);
    border: 1px solid var(--divider);
    border-radius: 4px;
  }
  kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 20px;
    height: 21px;
    padding: 0 5px;
    margin: 0 1px;
    font-size: 11px;
    font-family: inherit;
    background: var(--cap-bg);
    color: var(--cap-fg);
    border: 1px solid var(--cap-border);
    border-radius: 4px;
    box-shadow: 0 1px 0 var(--cap-border);
  }
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
  .plus {
    color: var(--muted);
    font-size: 11px;
    margin: 0 1px;
  }
</style>
