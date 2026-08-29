import { Icon, isIconName } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type ConnectorType, type McpServerInfo, core } from "../lib/core";
import { ModalShell } from "./ModalShell";

type ConnectorFilter = "all" | "connection" | "mcp";

function isMcpType(type: ConnectorType): boolean {
  return type.category === "extension" || type.type_id.startsWith("mcp");
}

function connectorCategoryLabel(type: ConnectorType): string {
  return isMcpType(type) ? "Serveur MCP" : "Connecteur";
}

export function ConnectorsSettings() {
  const [tab, setTab] = useState<"browse" | "installed">("browse");
  const [categoryFilter, setCategoryFilter] = useState<ConnectorFilter>("all");
  const [types, setTypes] = useState<ConnectorType[]>([]);

  // Custom MCP Modal state
  const [mcpFormOpen, setMcpFormOpen] = useState(false);
  const [mcpName, setMcpName] = useState("");
  const [mcpType, setMcpType] = useState<"stdio" | "http">("stdio");
  const [mcpCommand, setMcpCommand] = useState("");
  const [mcpServers, setMcpServers] = useState<McpServerInfo[]>([]);
  const [mcpBusy, setMcpBusy] = useState<string | null>(null);
  const [mcpError, setMcpError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setMcpServers(await core.listMcpServers());
    } catch {
      setMcpServers([]);
    }
  }, []);

  useEffect(() => {
    core
      .listConnectorTypes()
      .then(setTypes)
      .catch(() => setTypes([]));
    refresh();
  }, [refresh]);

  async function saveCustomMcp() {
    const name = mcpName.trim();
    const target = mcpCommand.trim();
    if (!name || !target) return;
    setMcpBusy("__add__");
    setMcpError(null);
    try {
      setMcpServers(await core.addMcpServer({ name, transport: mcpType, target, autoStart: true }));
      try {
        await core.startMcpServer(name);
      } catch (e) {
        setMcpError(String(e));
      }
      setMcpServers(await core.listMcpServers());
      setMcpFormOpen(false);
      setMcpName("");
      setMcpCommand("");
      setTab("installed");
    } catch (e) {
      setMcpError(String(e));
    } finally {
      setMcpBusy(null);
    }
  }

  async function toggleMcp(server: McpServerInfo) {
    setMcpBusy(server.name);
    setMcpError(null);
    try {
      if (server.running) {
        await core.stopMcpServer(server.name);
      } else {
        await core.startMcpServer(server.name);
      }
      setMcpServers(await core.listMcpServers());
    } catch (e) {
      setMcpError(String(e));
    } finally {
      setMcpBusy(null);
    }
  }

  async function removeMcp(server: McpServerInfo) {
    if (!window.confirm(`Retirer « ${server.name} » ?`)) return;
    setMcpBusy(server.name);
    try {
      setMcpServers(await core.removeMcpServer(server.name));
    } catch (e) {
      setMcpError(String(e));
    } finally {
      setMcpBusy(null);
    }
  }

  const connectorTypes = types.filter((t) => t.category === "connector" || isMcpType(t));
  const filteredTypes = connectorTypes.filter((t) => {
    if (categoryFilter === "all") return true;
    return categoryFilter === "mcp" ? isMcpType(t) : !isMcpType(t);
  });
  const activeCount = mcpServers.length;

  return (
    <div className="locaryn-conn-settings">
      <div className="locaryn-connector-intro">
        <div>
          <h3>Connecteurs &amp; Serveurs MCP</h3>
          <p>
            Les <strong>Connecteurs &amp; Serveurs MCP</strong> exposent des outils, contextes et
            accès de données externes (bases de données, fichiers, APIs distantes) directement aux
            modèles d'IA via le standard Model Context Protocol. Contrairement aux <em>Plugins</em>,
            ils n'injectent pas d'écrans ou de composants graphiques dans l'application hôte.
          </p>
        </div>
        <div className="locaryn-connector-legend" aria-label="Familles de connecteurs">
          <span title="Passerelle technique d'outils pour les modèles de langage">
            <strong>Serveur MCP</strong> · outils exposés à l'agent
          </span>
          <span title="Accès réseau ou machine sans modification d'UI">
            <strong>Connecteur</strong> · pont de données ou service
          </span>
        </div>
      </div>
      <div
        className="locaryn-store-tabs"
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: "16px",
        }}
      >
        <div style={{ display: "flex", gap: "8px" }}>
          <button
            type="button"
            className={`locaryn-tab-btn${tab === "browse" ? " locaryn-active" : ""}`}
            onClick={() => setTab("browse")}
          >
            Catalogue
          </button>
          <button
            type="button"
            className={`locaryn-tab-btn${tab === "installed" ? " locaryn-active" : ""}`}
            onClick={() => setTab("installed")}
          >
            Configurés ({activeCount})
          </button>
        </div>

        {tab === "browse" && (
          <div style={{ display: "flex", gap: "6px" }}>
            <button
              type="button"
              className={`locaryn-filter-btn${categoryFilter === "all" ? " locaryn-active" : ""}`}
              onClick={() => setCategoryFilter("all")}
            >
              Tous
            </button>
            <button
              type="button"
              className={`locaryn-filter-btn${categoryFilter === "mcp" ? " locaryn-active" : ""}`}
              onClick={() => setCategoryFilter("mcp")}
            >
              Serveurs MCP
            </button>
            <button
              type="button"
              className={`locaryn-filter-btn${categoryFilter === "connection" ? " locaryn-active" : ""}`}
              onClick={() => setCategoryFilter("connection")}
            >
              Connexions
            </button>
          </div>
        )}
      </div>

      {tab === "browse" ? (
        <>
          <div className="locaryn-connector-add-row" style={{ marginBottom: "16px" }}>
            <button
              type="button"
              className="locaryn-btn-primary"
              onClick={() => {
                setMcpError(null);
                setMcpFormOpen(true);
              }}
            >
              + Ajouter un serveur MCP personnalisé…
            </button>
          </div>

          <div className="locaryn-model-grid">
            {filteredTypes.map((t) => {
              const isMcp = isMcpType(t);
              return (
                <div key={t.type_id} className="locaryn-box-card">
                  <div className="locaryn-box-head">
                    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                      <span className="locaryn-connector-icon">
                        {/* Le service renvoie un nom du jeu ; tout le reste
                            retombe sur la prise, jamais sur du texte brut. */}
                        <Icon name={isIconName(t.icon) ? t.icon : "extensions"} size={16} />
                      </span>
                      <div>
                        <h3 className="locaryn-box-name">{t.display_name}</h3>
                        <span className="locaryn-box-brand">{connectorCategoryLabel(t)}</span>
                      </div>
                    </div>
                    {t.source === "built-in" && (
                      <span className="locaryn-badge-builtin">Intégré</span>
                    )}
                  </div>
                  <p className="locaryn-box-desc">{t.summary}</p>
                  {t.install_hint && (
                    <code className="locaryn-connector-cmd" title="Commande par défaut">
                      {t.install_hint}
                    </code>
                  )}

                  <div
                    style={{
                      marginTop: "auto",
                      paddingTop: "12px",
                      borderTop: "1px solid var(--border)",
                      display: "flex",
                      justifyContent: "flex-end",
                    }}
                  >
                    {isMcp ? (
                      <button
                        type="button"
                        className="locaryn-btn-primary"
                        onClick={() => {
                          setMcpError(null);
                          setMcpFormOpen(true);
                        }}
                      >
                        + Configurer un serveur MCP
                      </button>
                    ) : (
                      <button type="button" className="locaryn-btn-ghost" disabled>
                        Bientôt disponible
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </>
      ) : (
        <div>
          <h3 style={{ fontSize: "var(--text-md)", marginBottom: "12px" }}>
            Serveurs MCP ({mcpServers.length})
          </h3>
          {activeCount === 0 ? (
            <div className="locaryn-card" style={{ padding: 20, marginBottom: 28 }}>
              <strong>Aucun serveur MCP configuré</strong>
              <p className="locaryn-field-hint" style={{ margin: "6px 0 14px" }}>
                Une carte du catalogue décrit une possibilité ; elle n'est comptée ici qu'après
                l'enregistrement d'un serveur MCP.
              </p>
              <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                <button
                  type="button"
                  className="locaryn-btn-primary"
                  onClick={() => setTab("browse")}
                >
                  Parcourir le catalogue
                </button>
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  onClick={() => {
                    setMcpError(null);
                    setMcpFormOpen(true);
                  }}
                >
                  Ajouter un serveur MCP
                </button>
              </div>
            </div>
          ) : null}

          {mcpServers.length > 0 && (
            <div className="locaryn-model-grid" style={{ marginBottom: "28px" }}>
              {mcpServers.map((m) => (
                <div key={m.name} className="locaryn-box-card">
                  <div className="locaryn-box-head">
                    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                      <span
                        className={`locaryn-health-dot ${
                          m.running ? "locaryn-health-ok" : "locaryn-health-off"
                        }`}
                      />
                      <div>
                        <h3 className="locaryn-box-name">{m.name}</h3>
                        <span className="locaryn-box-brand">{m.transport}</span>
                      </div>
                    </div>
                  </div>
                  <code className="locaryn-connector-cmd">{m.target}</code>
                  <p className="locaryn-box-desc">
                    {m.running
                      ? m.tools.length > 0
                        ? `${m.tools.length} outil${m.tools.length > 1 ? "s" : ""} : ${m.tools.slice(0, 4).join(", ")}${m.tools.length > 4 ? "…" : ""}`
                        : "Démarré, mais ce serveur n'annonce aucun outil."
                      : "Arrêté."}
                  </p>
                  <div
                    style={{
                      marginTop: "auto",
                      paddingTop: "12px",
                      borderTop: "1px solid var(--border)",
                      display: "flex",
                      justifyContent: "space-between",
                    }}
                  >
                    <button
                      type="button"
                      className="locaryn-btn-ghost"
                      style={{ color: "var(--danger)", fontSize: "12px" }}
                      disabled={mcpBusy === m.name}
                      onClick={() => removeMcp(m)}
                    >
                      Retirer
                    </button>
                    <button
                      type="button"
                      className={`locaryn-btn-${m.running ? "ghost" : "primary"}`}
                      disabled={mcpBusy === m.name}
                      onClick={() => toggleMcp(m)}
                    >
                      {mcpBusy === m.name ? "…" : m.running ? "Arrêter" : "Démarrer"}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
          {mcpError && (
            <div className="locaryn-vp-error" style={{ marginBottom: "28px" }}>
              {mcpError}
            </div>
          )}
        </div>
      )}

      {/* Custom MCP Modal */}
      {mcpFormOpen && (
        <ModalShell
          label="Ajouter un serveur MCP"
          onClose={() => setMcpFormOpen(false)}
          style={{ maxWidth: 540 }}
        >
          {mcpError && <div className="locaryn-vp-error">{mcpError}</div>}
          <div className="locaryn-field">
            <label htmlFor="mcp-name" className="locaryn-field-label">
              Nom du serveur MCP
            </label>
            <input
              id="mcp-name"
              className="locaryn-input"
              placeholder="graphify"
              value={mcpName}
              onChange={(e) => setMcpName(e.target.value)}
            />
            <p className="locaryn-field-hint">
              Ce nom préfixe les outils vus par le modèle : lettres, chiffres, « - » et « _ »
              uniquement.
            </p>
          </div>
          <div className="locaryn-field">
            <label htmlFor="mcp-transport" className="locaryn-field-label">
              Protocole Transport
            </label>
            <select
              id="mcp-transport"
              className="locaryn-select"
              value={mcpType}
              onChange={(e) => setMcpType(e.target.value as "stdio" | "http")}
            >
              <option value="stdio">Commande locale (npx, uvx, python…)</option>
              <option value="http">Adresse HTTP</option>
            </select>
          </div>
          <div className="locaryn-field">
            <label htmlFor="mcp-target" className="locaryn-field-label">
              {mcpType === "stdio" ? "Commande à lancer" : "Adresse du serveur"}
            </label>
            <input
              id="mcp-target"
              className="locaryn-input"
              placeholder={
                mcpType === "stdio"
                  ? "npx -y @modelcontextprotocol/server-filesystem D:/Documents"
                  : "https://exemple.com/mcp"
              }
              value={mcpCommand}
              onChange={(e) => setMcpCommand(e.target.value)}
            />
          </div>
          <div
            className="locaryn-field-actions"
            style={{ marginTop: "16px", display: "flex", gap: "8px", justifyContent: "flex-end" }}
          >
            <button
              type="button"
              className="locaryn-btn-ghost"
              onClick={() => setMcpFormOpen(false)}
            >
              Annuler
            </button>
            <button
              type="button"
              className="locaryn-btn-primary"
              disabled={mcpBusy === "__add__"}
              onClick={saveCustomMcp}
            >
              {mcpBusy === "__add__" ? "Démarrage…" : "Enregistrer et démarrer"}
            </button>
          </div>
        </ModalShell>
      )}
    </div>
  );
}
