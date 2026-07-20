<script lang="ts">
  // Transparent, click-through window drawing numbered chips over taskbar
  // buttons. Positions come from the agent's taskbar snapshot, forwarded by
  // the app as absolute window-local coordinates.
  import { onOverlayState, onAppearance, getAppearance } from "../lib/ipc";
  import { applyAppearance } from "../lib/theme";

  interface Badge {
    label: string;
    x: number;
    y: number;
  }
  let badges = $state<Badge[]>([]);
  // Positions arrive in physical px. devicePixelRatio is the page's true
  // render scale — monitor DPI *and* WebView2's zoom for Windows text
  // scaling, which the Rust side cannot see — so divide here.
  let dpr = $state(window.devicePixelRatio || 1);

  $effect(() => {
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/event")
      .then((ev) =>
        ev.listen<Badge[]>("badges://positions", (e) => {
          dpr = window.devicePixelRatio || 1;
          badges = e.payload;
        }),
      )
      .then((f) => (unlisten = f))
      .catch(() => {});
    const unlistenAppearance = onAppearance((a) => applyAppearance(a));
    getAppearance().then((a) => a && applyAppearance(a));
    return () => {
      unlisten?.();
      unlistenAppearance.then((f) => f());
    };
  });
  // Reference the import so tree-shaking keeps the shared ipc module tidy.
  void onOverlayState;
</script>

{#each badges as b (b.label + b.x)}
  <div class="badge" style="left:{b.x / dpr}px; top:{b.y / dpr}px">{b.label}</div>
{/each}

<style>
  :global(body) {
    background: transparent;
  }
  /* Fixed colors, no theming: the chips float over the *taskbar* (whose
     color tracks the Windows theme, not the panel's), so they must read
     against anything. Near-black pill + white digit, with a white inner
     ring and a dark outer halo so the edge survives dark and light bars. */
  .badge {
    position: absolute;
    transform: translate(-50%, -50%);
    min-width: 22px;
    height: 22px;
    padding: 0 5px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
    font-weight: 700;
    color: #fff;
    background: rgba(17, 17, 17, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.92);
    border-radius: 11px;
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.6),
      0 2px 6px rgba(0, 0, 0, 0.5);
  }
</style>
