import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ConnectIntentModal } from "./components/ConnectIntentModal";
import "./lib/pluginBridge";
import "./styles/global.css";

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");
createRoot(root).render(
  <StrictMode>
    <App />
    {/* Demande de connexion `locaryn://connect` : au-dessus de tout, gate
        d'authentification compris — le .exe du serveur peut ouvrir une app
        pas encore connectée, et la demande doit rester visible. */}
    <ConnectIntentModal />
  </StrictMode>,
);
