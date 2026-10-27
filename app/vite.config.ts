import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The frontend lives in ui/; Tauri loads the build from ../dist (see
// src-tauri/tauri.conf.json).
export default defineConfig({
  root: "ui",
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: "../dist", emptyOutDir: true, target: "es2022" },
});
