import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      // Le même jeu d'icônes que l'application de bureau : un seul dessin par
      // idée, dans les deux interfaces. Chemin absolu : en relatif, la
      // compilation passe mais le serveur de développement ne résout rien.
      "@locaryn/ui-core": fileURLToPath(
        new URL("../../packages-ui/core/src/index.tsx", import.meta.url),
      ),
    },
  },
  clearScreen: false,
  // A phone reaches the dev server across the network, so unlike the desktop
  // one it cannot be bound to loopback.
  server: { port: 1422, strictPort: true, host: "0.0.0.0" },
});
