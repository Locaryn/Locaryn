import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      // Le même socle partagé que l'ordinateur et le téléphone : les icônes,
      // le thème, et les quatre formes de chargement. Chemin absolu : en
      // relatif, la compilation passe mais le serveur de développement ne
      // résout rien.
      "@locaryn/ui-core": fileURLToPath(
        new URL("../../packages-ui/core/src/index.tsx", import.meta.url),
      ),
    },
  },
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
