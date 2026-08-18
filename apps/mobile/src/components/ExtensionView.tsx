import { Icon, isIconName } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type PhoneExtension, api } from "../lib/core";
import { Screen } from "./Screen";

type Props = {
  screenId: string;
  onBack: () => void;
  onOpenChat: (initialText?: string) => void;
};

export function ExtensionView({ screenId, onBack, onOpenChat }: Props) {
  const [extensions, setExtensions] = useState<PhoneExtension[]>([]);
  const [loading, setLoading] = useState(true);
  const [toolInput, setToolInput] = useState("");
  const [toolOutput, setToolOutput] = useState<string | null>(null);
  const [busyTool, setBusyTool] = useState<string | null>(null);
  const [toolError, setToolError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const exts = await api.listExtensions();
      setExtensions(exts);
    } catch {
      setExtensions([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // Trouver l'extension qui apporte ce screenId ou qui correspond au nom
  const extension = extensions.find(
    (e) =>
      e.name === screenId ||
      (e.ui?.nav_items ?? []).some((ni) => ni.id === screenId) ||
      (e.ui?.studio_tabs ?? []).some((st) => st.id === screenId),
  );

  const navItem = extension?.ui?.nav_items?.find((ni) => ni.id === screenId);
  const title = navItem?.label ?? extension?.display_name ?? screenId;

  async function handleRunTool(toolName: string) {
    if (!toolInput.trim()) return;
    setBusyTool(toolName);
    setToolError(null);
    setToolOutput(null);
    try {
      const result = await api.runComposerTool(toolName, toolInput.trim());
      setToolOutput(result);
    } catch (e) {
      setToolError(String(e));
    } finally {
      setBusyTool(null);
    }
  }

  async function copyOutput() {
    if (!toolOutput) return;
    try {
      await navigator.clipboard.writeText(toolOutput);
      setNotice("Résultat copié !");
      window.setTimeout(() => setNotice(null), 2500);
    } catch {
      // fallback
    }
  }

  return (
    <Screen title={title} onBack={onBack}>
      {notice && (
        <div className="lo-toast">
          <p className="lo-notice">{notice}</p>
        </div>
      )}

      {loading && <p className="lo-sub">Chargement de l'extension…</p>}

      {!loading && !extension && (
        <div className="lo-card" style={{ flexDirection: "column", alignItems: "stretch" }}>
          <span className="lo-card-title">{screenId}</span>
          <p className="lo-hint">
            Cette vue correspond à une extension qui n'est actuellement pas active ou introuvable sur le serveur.
          </p>
          <button type="button" className="lo-btn-ghost" style={{ marginTop: 12 }} onClick={onBack}>
            Revenir au menu
          </button>
        </div>
      )}

      {!loading && extension && (
        <>
          {/* Carte d'information de l'extension */}
          <div className="lo-card" style={{ flexDirection: "column", alignItems: "stretch" }}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
              <div>
                <span className="lo-card-title">{extension.display_name || extension.name}</span>
                <span style={{ fontSize: 11, color: "var(--text-faint)", marginLeft: 6 }}>
                  v{extension.version}
                </span>
                {extension.description && (
                  <p className="lo-hint" style={{ marginTop: 4 }}>
                    {extension.description}
                  </p>
                )}
              </div>
              <span
                style={{
                  fontSize: 11,
                  padding: "2px 8px",
                  borderRadius: 10,
                  background: extension.enabled ? "rgba(var(--accent-rgb), 0.2)" : "rgba(255, 255, 255, 0.08)",
                  color: extension.enabled ? "var(--accent)" : "var(--text-faint)",
                  fontWeight: 600,
                  flex: "none",
                }}
              >
                {extension.enabled ? "Active" : "Désactivée"}
              </span>
            </div>

            {extension.capabilities.length > 0 && (
              <div style={{ display: "flex", gap: 4, flexWrap: "wrap", marginTop: 8 }}>
                {extension.capabilities.map((c) => (
                  <span
                    key={c}
                    style={{
                      fontSize: 11,
                      padding: "2px 6px",
                      background: "rgba(var(--accent-rgb), 0.1)",
                      color: "var(--accent)",
                      borderRadius: 4,
                    }}
                  >
                    {c}
                  </span>
                ))}
              </div>
            )}
          </div>

          {/* Outils & Actions disponibles dans cette extension */}
          {extension.ui?.composer_actions && extension.ui.composer_actions.length > 0 && (
            <section className="lo-section" style={{ marginTop: "var(--space-3)" }}>
              <h2 className="lo-section-title">Outils et Actions de l'extension</h2>
              <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
                <div>
                  <label className="lo-label">Texte / Données d'entrée pour l'outil</label>
                  <textarea
                    className="lo-input"
                    rows={3}
                    placeholder="Saisissez le texte ou les paramètres à envoyer à l'outil…"
                    value={toolInput}
                    onChange={(e) => setToolInput(e.target.value)}
                  />
                </div>

                <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                  {extension.ui.composer_actions.map((act) => (
                    <button
                      key={act.id}
                      type="button"
                      className="lo-btn-small lo-btn-small-on"
                      disabled={busyTool !== null || !toolInput.trim()}
                      onClick={() => void handleRunTool(act.value)}
                    >
                      <Icon name={act.icon && isIconName(act.icon) ? act.icon : "extensions"} size={14} />
                      <span>{busyTool === act.value ? "Exécution…" : act.label}</span>
                    </button>
                  ))}
                </div>

                {toolError && <p className="lo-error">{toolError}</p>}

                {toolOutput && (
                  <div
                    style={{
                      marginTop: 8,
                      padding: 12,
                      background: "rgba(0, 0, 0, 0.3)",
                      border: "1px solid var(--border)",
                      borderRadius: "var(--radius)",
                    }}
                  >
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 6 }}>
                      <span style={{ fontSize: 12, fontWeight: 700, color: "var(--accent)" }}>Résultat</span>
                      <div style={{ display: "flex", gap: 6 }}>
                        <button type="button" className="lo-btn-small" onClick={copyOutput}>
                          Copier
                        </button>
                        <button
                          type="button"
                          className="lo-btn-small"
                          onClick={() => onOpenChat(toolOutput)}
                        >
                          Envoyer au Chat
                        </button>
                      </div>
                    </div>
                    <pre className="lo-code-block">{toolOutput}</pre>
                  </div>
                )}
              </div>
            </section>
          )}

          {/* Bouton pour démarrer une conversation avec cette extension */}
          <div style={{ marginTop: "var(--space-4)" }}>
            <button
              type="button"
              className="lo-btn"
              onClick={() => onOpenChat(`Utiliser l'extension ${extension.display_name || extension.name} : `)}
            >
              Ouvrir dans le Chat
            </button>
          </div>
        </>
      )}
    </Screen>
  );
}
