import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri serves the built files itself; the dev server is only for `tauri dev`.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  // The translations live at the repository root (shared with `pco`).
  server: { port: 5173, strictPort: true, fs: { allow: [".", "../i18n"] } },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
  },
  // Svelte components are tested as they run in the browser.
  resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});
