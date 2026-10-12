import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Im Entwicklungsmodus leitet Vite API und WebSocket an den DeckLight-Kern weiter.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": "http://127.0.0.1:8750",
      "/ws": { target: "ws://127.0.0.1:8750", ws: true },
    },
  },
  build: { target: "es2022", outDir: "dist" },
});
