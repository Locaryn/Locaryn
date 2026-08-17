import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // The web client is served by the daemon in production; this dev server is
  // only for working on the interface, so loopback is fine. Sans proxy, les
  // appels /v1 tombaient sur le serveur Vite lui-même : le daemon, qui sert
  // déjà l'API sur 127.0.0.1:7474, est le relais naturel en développement.
  server: {
    port: 1423,
    strictPort: true,
    proxy: { "/v1": "http://127.0.0.1:7474" },
  },
});
