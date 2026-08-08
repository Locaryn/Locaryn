import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@locaryn/ui-core": "../../packages-ui/core/src/index.tsx",
      "@locaryn/ui-chat": "../../packages-ui/chat/src/index.tsx",
      "@locaryn/ui-preview": "../../packages-ui/preview/src/index.tsx",
      "@locaryn/ui-terminal": "../../packages-ui/terminal/src/index.tsx",
    },
  },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
});
