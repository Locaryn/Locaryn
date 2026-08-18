import { useCallback, useEffect, useState } from "react";
import {
  type ConnectorType,
  type McpServerInfo,
  type SshAiAccess,
  type SshServer,
  core,
} from "../lib/core";
import { ModalShell } from "./ModalShell";
import { SshServerForm } from "./ssh/SshServerForm";

const AI_ACCESS_OPTIONS: { value: SshAiAccess; label: string }[] = [
  { value: "none", label: "Invisible pour l'IA" },
  { value: "read_only", label: "Lecture seule" },
  { value: "approval", label: "Demander confirmation" },
  { value: "trusted", label: "Confiance totale" },
];

type ConnectorFilter = "all" | "connection" | "mcp";

function isMcpType(type: ConnectorType): boolean {
  // Le daemon appelle encore cette famille `extension` pour compatibilité de
  // contrat ; dans l'interface, elle reste explicitement un serveur MCP et
  // ne doit jamais être confondue avec une extension Locaryn.
  return type.category === "extension" || type.type_id.startsWith("mcp");
}

function connectorCategoryLabel(type: ConnectorType): string {
  return isMcpType(type) ? "Serveur MCP" : "Connecteur";
}

export function ConnectorsSettings() {
  const [tab, setTab] = useState<"browse" | "installed">("browse");
  const [categoryFilter, setCategoryFilter] = useState<ConnectorFilter>("all");
  const [types, setTypes] = useState<ConnectorType[]>([]);
  const [servers, setServers] = useState<SshServer[]>([]);
  // Cette vue ne déduit jamais un état « installé » d'une carte du catalogue :
  // seules les connexions SSH enregistrées et les serveurs MCP présents dans
  // mcp.json sont des éléments configurés. Cela évite les compteurs fantômes
  // et les cartes actives vides après un redémarrage.
  const [sshFormOpen, setSshFormOpen] = useState(false);

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
      setServers(await core.listSshServers());
    } catch {
      setServers([]);
    }
    try {
      setMcpServers(await core.listMcpServers());
    } catch {
      /* Rien d'enregistré, ou mcp.json illisible : la vue reste exploitable
         et une erreur apparaîtra lors de l'action qui échoue réellement. */
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

  async function changeAiAccess(server: SshServer, level: SshAiAccess) {
    await core.setSshAiAccess(server.id, level);
    refresh();
  }

  async function removeSsh(server: SshServer) {
    if (!window.confirm(`Supprimer "${server.name}" ?`)) return;
    await core.deleteSshServer(server.id);
    refresh();
  }

  /**
   * Register the server, then start it straight away.
   *
   * Starting is what proves the command is right: the transport is lazy, so
   * a typo would otherwise show up as a chat that quietly has no tools.
   */
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
        // Registered but not reachable: keep it, say why. Removing it would
        // throw away a command that is one character from working.
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

  // Cette page ne montre que deux familles : les connexions (SSH, bases,
  // services) et les serveurs MCP. Les plugins de fonctionnalités (LSP,
  // Python, Playwright…) vivent dans Extensions, jamais ici.
  const connectorTypes = types.filter((t) => t.category === "connector" || isMcpType(t));
  const filteredTypes = connectorTypes.filter((t) => {
    if (categoryFilter === "all") return true;
    return categoryFilter === "mcp" ? isMcpType(t) : !isMcpType(t);
  });
  // Un type de catalogue décrit une possibilité, pas une installation. Les
  // seules installations réelles sont les serveurs persistés ci-dessous.
  const activeCount = mcpServers.length + servers.length;

  return (
    <div className="locaryn-conn-settings">
      <div className="locaryn-connector-intro">
        <div>
          <h3>Connecteurs &amp; serveurs MCP</h3>
          <p>
            Les connecteurs donnent accès à une machine ou à un service. Les serveurs MCP exposent
            des outils à l'agent. Les extensions et plugins qui ajoutent des fonctionnalités à
            l'application sont gérés séparément dans « Extensions ».
          </p>
        </div>
        <div className="locaryn-connector-legend" aria-label="Familles de connecteurs">
          <span>
            <strong>Connecteur</strong> · accès à un service
          </span>
          <span>
            <strong>Serveur MCP</strong> · outils exposés à l'agent
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

        <div style={{ display: "flex", gap: "8px" }}>
          <button
            type="button"
            className="locaryn-btn-primary"
            style={{ fontSize: "12px", padding: "4px 12px" }}
            onClick={() => setMcpFormOpen(true)}
            title="Ajouter un serveur qui expose des outils via le protocole MCP"
          >
            + Ajouter un serveur MCP
          </button>
        </div>
      </div>

      {tab === "browse" ? (
        <>
          <div className="locaryn-size-chips" style={{ marginBottom: "16px" }}>
            <button
              type="button"
              className={`locaryn-chip${categoryFilter === "all" ? " locaryn-chip-on" : ""}`}
              onClick={() => setCategoryFilter("all")}
            >
              Tous
            </button>
            <button
              type="button"
              className={`locaryn-chip${categoryFilter === "connection" ? " locaryn-chip-on" : ""}`}
              onClick={() => setCategoryFilter("connection")}
            >
              Connecteurs
            </button>
            <button
              type="button"
              className={`locaryn-chip${categoryFilter === "mcp" ? " locaryn-chip-on" : ""}`}
              onClick={() => setCategoryFilter("mcp")}
            >
              Serveurs MCP
            </button>
          </div>

          <div className="locaryn-model-grid">
            {filteredTypes.map((t) => {
              const mcpType = isMcpType(t);
              return (
                <div key={t.type_id} className="locaryn-box-card" style={{ minHeight: "180px" }}>
                  <div className="locaryn-box-head">
                    <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                      <span style={{ fontSize: "24px" }} aria-hidden="true">
                        {t.icon}
                      </span>
                      <div>
                        <h3 className="locaryn-box-name">{t.display_name}</h3>
                        <span className="locaryn-box-brand">{t.source}</span>
                      </div>
                    </div>
                    <span className="locaryn-tag">{connectorCategoryLabel(t)}</span>
                  </div>

                  <p className="locaryn-box-desc">{t.summary}</p>
                  {t.install_hint && (
                    <code
                      className="locaryn-connector-cmd"
                      title="Commande qui lance ce serveur MCP"
                    >
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
                    {t.type_id === "ssh" ? (
                      <button
                        type="button"
                        className="locaryn-btn-primary"
                        onClick={() => setSshFormOpen(true)}
                      >
                        + Ajouter une connexion SSH
                      </button>
                    ) : mcpType ? (
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
            Connecteurs configurés ({servers.length}) · Serveurs MCP ({mcpServers.length})
          </h3>
          {activeCount === 0 ? (
            <div className="locaryn-card" style={{ padding: 20, marginBottom: 28 }}>
              <strong>Aucun connecteur ou serveur MCP configuré</strong>
              <p className="locaryn-field-hint" style={{ margin: "6px 0 14px" }}>
                Une carte du catalogue décrit une possibilité ; elle n'est comptée ici qu'après
                l'ajout réel d'une connexion ou l'enregistrement d'un serveur MCP.
              </p>
              <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                <button
                  type="button"
                  className="locaryn-btn-primary"
                  onClick={() => setTab("browse")}
                >
                  Parcourir les connecteurs
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

          <h3 style={{ fontSize: "var(--text-md)", marginBottom: "12px" }}>
            Serveurs MCP ({mcpServers.length})
          </h3>
          {mcpServers.length === 0 ? (
            <p className="locaryn-field-hint" style={{ marginBottom: "28px" }}>
              Aucun serveur MCP. Ajoutez-en un depuis l'onglet « Parcourir » — n'importe quel
              serveur du protocole convient, y compris ceux prévus pour Claude Code ou Cursor.
            </p>
          ) : (
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

          {servers.length > 0 && (
            <>
              <h3 style={{ fontSize: "var(--text-md)", marginBottom: "12px" }}>
                Serveurs SSH enregistrés ({servers.length})
              </h3>
              <div className="locaryn-model-grid">
                {servers.map((s) => (
                  <div key={s.id} className="locaryn-box-card">
                    <div className="locaryn-box-head">
                      <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                        <span
                          className={`locaryn-health-dot ${
                            s.status === "ok" ? "locaryn-health-ok" : "locaryn-health-off"
                          }`}
                        />
                        <div>
                          <h3 className="locaryn-box-name">{s.name}</h3>
                          <span className="locaryn-box-brand">
                            {s.username}@{s.host}:{s.port}
                          </span>
                        </div>
                      </div>
                    </div>
                    <p className="locaryn-box-desc">{s.description || "Serveur SSH distant"}</p>

                    <div
                      style={{
                        marginTop: "auto",
                        paddingTop: "12px",
                        borderTop: "1px solid var(--border)",
                        display: "flex",
                        flexDirection: "column",
                        gap: "8px",
                      }}
                    >
                      <div
                        style={{
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "space-between",
                        }}
                      >
                        <span style={{ fontSize: "11px", color: "var(--text-faint)" }}>
                          Accès IA :
                        </span>
                        <select
                          className="locaryn-select locaryn-select-sm"
                          value={s.ai_access}
                          onChange={(e) => changeAiAccess(s, e.target.value as SshAiAccess)}
                        >
                          {AI_ACCESS_OPTIONS.map((o) => (
                            <option key={o.value} value={o.value}>
                              {o.label}
                            </option>
                          ))}
                        </select>
                      </div>
                      <button
                        type="button"
                        className="locaryn-btn-ghost"
                        style={{ color: "var(--danger)", fontSize: "12px", alignSelf: "flex-end" }}
                        onClick={() => removeSsh(s)}
                      >
                        Supprimer
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      )}

      {/* Form modal to add custom MCP server */}
      {mcpFormOpen && (
        <ModalShell
          onClose={() => setMcpFormOpen(false)}
          label="Ajouter un serveur MCP"
          className="locaryn-card locaryn-modal-card locaryn-mcp-modal"
        >
          <h3>Ajouter un serveur MCP</h3>
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

      {sshFormOpen && (
        <SshServerForm
          onClose={() => setSshFormOpen(false)}
          onSaved={() => {
            setSshFormOpen(false);
            setTab("installed");
            refresh();
          }}
        />
      )}
    </div>
  );
}
