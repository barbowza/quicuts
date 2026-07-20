import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Multi-page: each window (overlay, badges, settings) loads its own HTML.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "0.0.0.0",
  },
  build: {
    target: "esnext",
    rollupOptions: {
      input: {
        overlay: "overlay.html",
        badges: "badges.html",
        settings: "settings.html",
      },
    },
  },
});
