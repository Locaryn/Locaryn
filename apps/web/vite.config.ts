import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // The web client is served by the daemon in production; this dev server is
  // only for working on the interface, so loopback is fine.
  server: { port: 1423, strictPort: true },
});
