import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      // Le même jeu d'icônes que l'application de bureau : un seul dessin par
      // idée, dans les deux interfaces.
      "@locaryn/ui-core": "../../packages-ui/core/src/index.tsx",
    },
  },
  clearScreen: false,
  // A phone reaches the dev server across the network, so unlike the desktop
  // one it cannot be bound to loopback.
  server: { port: 1422, strictPort: true, host: "0.0.0.0" },
});
