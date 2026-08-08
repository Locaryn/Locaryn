import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@lochor/ui-core": "../../packages-ui/core/src/index.tsx",
      "@lochor/ui-chat": "../../packages-ui/chat/src/index.tsx",
      "@lochor/ui-preview": "../../packages-ui/preview/src/index.tsx",
      "@lochor/ui-terminal": "../../packages-ui/terminal/src/index.tsx",
    },
  },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
});
