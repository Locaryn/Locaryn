import { Presentation } from "@locaryn/ui-core";
import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { PRESENTATION_KEY, PRESENTATION_WEB } from "./lib/presentation";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
    <Presentation steps={PRESENTATION_WEB} storageKey={PRESENTATION_KEY} />
  </React.StrictMode>,
);
