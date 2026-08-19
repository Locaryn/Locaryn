import { Icon } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import type { UseThemeReturn } from "../hooks/useTheme";
import { type AppInfo, core } from "../lib/core";
import { ModelBrowser } from "./ModelBrowser";
import { PerformancePanel } from "./PerformancePanel";

type Props = {
  theme: UseThemeReturn;
  /** Called after the active provider/model changes, so the app can refresh. */
  onProviderChanged?: () => void;
  /** Open the full-page application settings (everything, not just this chat). */
  onOpenFullSettings?: () => void;
};

type Tab = "provider" | "performance";
type Conn = "idle" | "testing" | "ok" | "error";

/** Managed llama-server port. NOT 11434 — that is Ollama's, and saving it made
 *  the app point at a server that isn't there ("Aucun modèle local n'a répondu"). */
const DEFAULT_ENDPOINT = "http://127.0.0.1:8080";

export function SettingsPanel({ theme, onProviderChanged, onOpenFullSettings }: Props) {
  const { settingsOpen, setSettingsOpen } = theme;
  const [tab, setTab] = useState<Tab>("provider");

  // Provider & model state
  const [endpoint, setEndpoint] = useState(DEFAULT_ENDPOINT);
  const [model, setModel] = useState("");
  const [models, setModels] = useState<string[]>([]);
  const [conn, setConn] = useState<Conn>("idle");
  const [connMsg, setConnMsg] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [modelView, setModelView] = useState<"server" | "browse">("server");

  // Close on Escape.
  // Load provider + app info on mount.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const providers = await core.listProviders();
        if (cancelled) return;
        const active = providers.find((p) => p.is_active) ?? providers[0];
        if (active) {
          setEndpoint(active.endpoint || DEFAULT_ENDPOINT);
          setModel(active.model ?? "");
        }
      } catch {
        // Keep defaults.
      }
      try {
        const i = await core.appInfo();
        if (!cancelled) setInfo(i);
      } catch {
        // About tab will show what it can.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // When catalogue opens, fetch models
  useEffect(() => {
    if (tab !== "provider" || modelView !== "browse") return;
    let cancelled = false;
    (async () => {
      try {
        const list = await core.listModels(endpoint.trim());
        if (!cancelled) setModels(list);
      } catch {
        // Offline
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [tab, modelView, endpoint]);

  async function refreshModels() {
    setConn("testing");
    setConnMsg("");
    try {
      const list = await core.listModels(endpoint.trim());
      setModels(list);
      setConn("ok");
      setConnMsg(
        list.length
          ? `connecté · ${list.length} modèle${list.length === 1 ? "" : "s"}`
          : "connecté · aucun modèle installé",
      );
      if ((!model || !list.includes(model)) && list.length) setModel(list[0]);
    } catch (e) {
      setModels([]);
      setConn("error");
      setConnMsg(String(e).replace(/^Error:\s*/, ""));
    }
  }

  async function save() {
    setSaving(true);
    setSaved(false);
    try {
      await core.configureProvider(endpoint.trim(), model.trim() || null);
      setSaved(true);
      onProviderChanged?.();
      window.setTimeout(() => setSaved(false), 1800);
    } catch (e) {
      setConn("error");
      setConnMsg(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setSaving(false);
    }
  }

  async function useCatalogModel(
    tag: string,
    _onProgress?: (pct: number) => void,
    _heretic?: boolean,
    consent?: boolean,
  ) {
    try {
      if (!models.includes(tag)) {
        await core.pullModel(endpoint.trim(), tag, undefined, undefined, consent);
        await refreshModels();
      }
      setModel(tag);
      await core.configureProvider(endpoint.trim(), tag);
      setSaved(true);
      onProviderChanged?.();
      setModelView("server");
      window.setTimeout(() => setSaved(false), 1800);
    } catch (e) {
      setConn("error");
      setConnMsg(String(e).replace(/^Error:\s*/, ""));
    }
  }

  // Keep the current model selectable even if it isn't in the fetched list.
  const modelOptions = model && !models.includes(model) ? [model, ...models] : models;

  return (
    <>
      {/* Ce fond n'a aucun enfant : le panneau est positionné à part. Il ferme
          au clic, et Échap ferme quel que soit l'élément qui a le focus. */}
      <div
        className="locaryn-settings-backdrop"
        role="presentation"
        onClick={() => setSettingsOpen(false)}
        onKeyDown={(e) => {
          if (e.key === "Escape") setSettingsOpen(false);
        }}
      />
      <dialog
        open
        className="locaryn-settings-modal"
        aria-modal="true"
        aria-label="Paramètres du chat"
      >
        <div className="locaryn-settings-header">
          <span className="locaryn-settings-title">Paramètres du chat</span>
          <button
            type="button"
            className="locaryn-settings-close"
            onClick={() => setSettingsOpen(false)}
            aria-label="Fermer les paramètres"
          >
            <Icon name="close" size={16} />
          </button>
        </div>

        <div className="locaryn-settings-main">
          <nav className="locaryn-settings-nav">
            <button
              type="button"
              className={`locaryn-nav-item${tab === "provider" ? " locaryn-active" : ""}`}
              onClick={() => setTab("provider")}
            >
              Modèle
            </button>
            <button
              type="button"
              className={`locaryn-nav-item${tab === "performance" ? " locaryn-active" : ""}`}
              onClick={() => setTab("performance")}
            >
              <Icon name="speed" size={15} /> Performance
            </button>
            {onOpenFullSettings && (
              <button
                type="button"
                className="locaryn-settings-all"
                onClick={() => {
                  setSettingsOpen(false);
                  onOpenFullSettings();
                }}
                title="Moteur, projets, extensions, apparence, stockage…"
              >
                Tous les paramètres →
              </button>
            )}
          </nav>

          <div className="locaryn-settings-pane">
            {tab === "performance" && <PerformancePanel />}

            {tab === "provider" && (
              <>
                <div className="locaryn-store-tabs">
                  <button
                    type="button"
                    className={`locaryn-tab-btn${modelView === "server" ? " locaryn-active" : ""}`}
                    onClick={() => setModelView("server")}
                  >
                    Serveur
                  </button>
                  <button
                    type="button"
                    className={`locaryn-tab-btn${modelView === "browse" ? " locaryn-active" : ""}`}
                    onClick={() => setModelView("browse")}
                  >
                    Parcourir les modèles
                  </button>
                </div>

                {modelView === "browse" ? (
                  <ModelBrowser onInstall={useCatalogModel} installed={models} />
                ) : (
                  <>
                    <div className="locaryn-field">
                      <label className="locaryn-field-label" htmlFor="locaryn-endpoint">
                        Serveur de modèles local
                      </label>
                      <div className="locaryn-field-row">
                        <input
                          id="locaryn-endpoint"
                          className="locaryn-input"
                          value={endpoint}
                          spellCheck={false}
                          autoCapitalize="off"
                          autoCorrect="off"
                          placeholder={DEFAULT_ENDPOINT}
                          onChange={(e) => {
                            setEndpoint(e.target.value);
                            setConn("idle");
                          }}
                        />
                        <button
                          type="button"
                          className="locaryn-btn-ghost"
                          onClick={refreshModels}
                          disabled={conn === "testing" || !endpoint.trim()}
                        >
                          {conn === "testing" ? "Test…" : "Tester"}
                        </button>
                      </div>
                      {conn !== "idle" && (
                        <div className={`locaryn-conn locaryn-conn-${conn}`}>
                          <span className="locaryn-conn-dot" />
                          {connMsg ||
                            (conn === "ok"
                              ? "connecté"
                              : conn === "testing"
                                ? "connecting…"
                                : "unreachable")}
                        </div>
                      )}
                      <p className="locaryn-field-hint">
                        Adresse du serveur de modèles local. Cliquez sur Tester pour lister les
                        modèles qu'il expose.
                      </p>
                    </div>

                    <div className="locaryn-field">
                      <label className="locaryn-field-label" htmlFor="locaryn-model">
                        Model
                      </label>
                      <select
                        id="locaryn-model"
                        className="locaryn-select"
                        value={model}
                        onChange={(e) => setModel(e.target.value)}
                      >
                        {modelOptions.length === 0 ? (
                          <option value="">— testez la connexion pour lister les modèles —</option>
                        ) : (
                          modelOptions.map((m) => (
                            <option key={m} value={m}>
                              {m}
                            </option>
                          ))
                        )}
                      </select>
                      <p className="locaryn-field-hint">
                        L'agent utilise ce modèle pour chaque message en mode local.
                      </p>
                    </div>

                    <div className="locaryn-field-actions">
                      <button
                        type="button"
                        className="locaryn-btn-primary"
                        onClick={save}
                        disabled={saving || !endpoint.trim()}
                      >
                        {saving ? "Enregistrement…" : saved ? "Enregistré" : "Enregistrer"}
                      </button>
                    </div>
                  </>
                )}
              </>
            )}
          </div>
        </div>
      </dialog>
    </>
  );
}
