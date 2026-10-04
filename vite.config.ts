import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'

// https://tauri.app/start/frontend/vite/
export default defineConfig({
  // Vite bails out on `process` access in dev; Tauri sets its own env vars.
  clearScreen: false,
  build: {
    rollupOptions: {
      // The tray panel (index.html) and the settings window (settings.html).
      input: {
        index: fileURLToPath(new URL('./index.html', import.meta.url)),
        settings: fileURLToPath(new URL('./settings.html', import.meta.url)),
      },
    },
  },
  server: {
    // Fail instead of silently picking another port — tauri.conf.json points here.
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
})
