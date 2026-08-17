import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

/**
 * Les paquets d'interface partagés, résolus en chemins **absolus**.
 *
 * En chemins relatifs, la compilation aboutissait — rollup les résout depuis
 * la racine — mais le serveur de développement échouait à chaque import avec
 * « Failed to resolve import ». L'application ne démarrait pas en dev, et la
 * panne est restée invisible tant qu'aucun fichier n'importait ces paquets.
 */
const partage = (chemin: string) =>
  fileURLToPath(new URL(`../../packages-ui/${chemin}`, import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@locaryn/ui-core": partage("core/src/index.tsx"),
      "@locaryn/ui-chat": partage("chat/src/index.tsx"),
      "@locaryn/ui-preview": partage("preview/src/index.tsx"),
      "@locaryn/ui-terminal": partage("terminal/src/index.tsx"),
    },
  },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
});
