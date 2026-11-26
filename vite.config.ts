import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    // IPv4 on purpose: "localhost" can bind to [::1] only on Windows, and the
    // Tauri webview then cannot reach the dev server.
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: { target: "es2022", chunkSizeWarningLimit: 900 },
});
