import { defineConfig } from "vite";

// https://tauri.app/start/frontend/vite/
export default defineConfig({
  // Vite bails out on `process` access in dev; Tauri sets its own env vars.
  clearScreen: false,
  server: {
    // Fail instead of silently picking another port — tauri.conf.json points here.
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
