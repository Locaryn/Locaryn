import { useEffect, useState } from "react";
import type { UseThemeReturn } from "../hooks/useTheme";
import { core, type AppInfo } from "../lib/core";
import { IMAGE_GEN_MODELS } from "../lib/modelRegistry";
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

  async function useCatalogModel(tag: string, _onProgress?: (pct: number) => void, _heretic?: boolean, consent?: boolean) {
    try {
      if (!models.includes(tag)) {
        await core.pullModel(endpoint.trim(), tag, undefined, undefined, consent);
        await refreshModels();
      }
      // If it's an image gen model (tag starts with x/ or is in IMAGE_GEN_MODELS), we just install it.
      // We don't want to set it as the active LLM text model.
      const isImage = IMAGE_GEN_MODELS.some(f => f.variants.some(v => v.tag === tag));
      if (tag.startsWith("x/") || isImage) {
        return;
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
      <div className="lochor-settings-backdrop" onClick={() => setSettingsOpen(false)} />
      <div
        className="lochor-settings-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
      >
        <div className="lochor-settings-header">
          <span className="lochor-settings-title">Paramètres du chat</span>
          <button
            type="button"
            className="lochor-settings-close"
            onClick={() => setSettingsOpen(false)}
            aria-label="Close settings"
          >
            ✕
          </button>
        </div>

        <div className="lochor-settings-main">
          <nav className="lochor-settings-nav">
            <button
              type="button"
              className={`lochor-nav-item${tab === "provider" ? " lochor-active" : ""}`}
              onClick={() => setTab("provider")}
            >
              Modèle
            </button>
            <button
              type="button"
              className={`lochor-nav-item${tab === "performance" ? " lochor-active" : ""}`}
              onClick={() => setTab("performance")}
            >
              ⚡ Performance
            </button>
            {onOpenFullSettings && (
              <button
                type="button"
                className="lochor-settings-all"
                onClick={() => { setSettingsOpen(false); onOpenFullSettings(); }}
                title="Moteur, projets, extensions, apparence, stockage…"
              >
                Tous les paramètres →
              </button>
            )}
          </nav>

          <div className="lochor-settings-pane">

            {tab === "performance" && <PerformancePanel />}


            {tab === "provider" && (
              <>
                <div className="lochor-store-tabs">
                  <button
                    type="button"
                    className={`lochor-tab-btn${modelView === "server" ? " lochor-active" : ""}`}
                    onClick={() => setModelView("server")}
                  >
                    Serveur
                  </button>
                  <button
                    type="button"
                    className={`lochor-tab-btn${modelView === "browse" ? " lochor-active" : ""}`}
                    onClick={() => setModelView("browse")}
                  >
                    Parcourir les modèles
                  </button>
                </div>

                {modelView === "browse" ? (
                  <ModelBrowser
                    onInstall={useCatalogModel}
                    installed={models}
                  />
                ) : (
                  <>
                <div className="lochor-field">
                  <label className="lochor-field-label" htmlFor="lochor-endpoint">
                    Serveur de modèles local
                  </label>
                  <div className="lochor-field-row">
                    <input
                      id="lochor-endpoint"
                      className="lochor-input"
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
                      className="lochor-btn-ghost"
                      onClick={refreshModels}
                      disabled={conn === "testing" || !endpoint.trim()}
                    >
                      {conn === "testing" ? "Test…" : "Tester"}
                    </button>
                  </div>
                  {conn !== "idle" && (
                    <div className={`lochor-conn lochor-conn-${conn}`}>
                      <span className="lochor-conn-dot" />
                      {connMsg ||
                        (conn === "ok"
                          ? "connecté"
                          : conn === "testing"
                            ? "connecting…"
                            : "unreachable")}
                    </div>
                  )}
                  <p className="lochor-field-hint">
                    Adresse du serveur de modèles local. Cliquez sur Tester pour lister
                    les modèles qu'il expose.
                  </p>
                </div>

                <div className="lochor-field">
                  <label className="lochor-field-label" htmlFor="lochor-model">
                    Model
                  </label>
                  <select
                    id="lochor-model"
                    className="lochor-select"
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
                  <p className="lochor-field-hint">
                    L'agent utilise ce modèle pour chaque message en mode local.
                  </p>
                </div>

                <div className="lochor-field-actions">
                  <button
                    type="button"
                    className="lochor-btn-primary"
                    onClick={save}
                    disabled={saving || !endpoint.trim()}
                  >
                    {saving ? "Enregistrement…" : saved ? "Enregistré ✓" : "Enregistrer"}
                  </button>
                </div>
                  </>
                )}
              </>
            )}


          </div>
        </div>
      </div>
    </>
  );
}
