import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // A phone reaches the dev server across the network, so unlike the desktop
  // one it cannot be bound to loopback.
  server: { port: 1422, strictPort: true, host: "0.0.0.0" },
});
