import { useCallback, useEffect, useState } from "react";
import {
  type ConnectorType,
  type McpServerInfo,
  type SshAiAccess,
  type SshServer,
  core,
} from "../lib/core";
import { ExtensionInstallDialog } from "./ExtensionInstallDialog";
import { ModalShell } from "./ModalShell";
import { SshServerForm } from "./ssh/SshServerForm";

const AI_ACCESS_OPTIONS: { value: SshAiAccess; label: string }[] = [
  { value: "none", label: "Invisible pour l'IA" },
  { value: "read_only", label: "Lecture seule" },
  { value: "approval", label: "Demander confirmation" },
  { value: "trusted", label: "Confiance totale" },
];

export function ConnectorsSettings() {
  const [tab, setTab] = useState<"browse" | "installed">("browse");
  const [categoryFilter, setCategoryFilter] = useState<string>("all");
  const [types, setTypes] = useState<ConnectorType[]>([]);
  const [servers, setServers] = useState<SshServer[]>([]);
  const [enabledIds, setEnabledIds] = useState<Set<string>>(
    new Set(["web_search", "memory_rag", "lsp"]),
  );
  const [sshFormOpen, setSshFormOpen] = useState(false);

  // Fenêtre d'ajout d'un plugin / extension (dépôt GitHub, dossier ou ZIP).
  const [extDialogOpen, setExtDialogOpen] = useState(false);

  // Custom MCP Modal state
  const [mcpFormOpen, setMcpFormOpen] = useState(false);
  const [mcpName, setMcpName] = useState("");
  const [mcpType, setMcpType] = useState<"stdio" | "http">("stdio");
  const [mcpCommand, setMcpCommand] = useState("");
  const [mcpServers, setMcpServers] = useState<McpServerInfo[]>([]);
  const [mcpBusy, setMcpBusy] = useState<string | null>(null);
  const [mcpError, setMcpError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setServers(await core.listSshServers());
    try {
      setMcpServers(await core.listMcpServers());
    } catch {
      /* nothing registered yet, or an unreadable mcp.json — the error
         surfaces on the next action rather than on a settings screen */
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

  function toggleEnable(typeId: string) {
    setEnabledIds((prev) => {
      const next = new Set(prev);
      if (next.has(typeId)) {
        next.delete(typeId);
      } else {
        next.add(typeId);
      }
      return next;
    });
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

  const filteredTypes = types.filter((t) => {
    if (categoryFilter === "all") return true;
    return t.category === categoryFilter;
  });

  return (
    <div className="locaryn-conn-settings">
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
            Store & Catalogue
          </button>
          <button
            type="button"
            className={`locaryn-tab-btn${tab === "installed" ? " locaryn-active" : ""}`}
            onClick={() => setTab("installed")}
          >
            Actifs / Installés ({enabledIds.size + servers.length})
          </button>
        </div>

        <div style={{ display: "flex", gap: "8px" }}>
          <button
            type="button"
            className="locaryn-btn-ghost"
            style={{ fontSize: "12px", padding: "4px 12px" }}
            onClick={() => setExtDialogOpen(true)}
            title="Installe un plugin ou une extension depuis un dépôt GitHub, un dossier local ou une archive ZIP"
          >
            + Plugin / Extension (dépôt, dossier, ZIP)
          </button>
          <button
            type="button"
            className="locaryn-btn-primary"
            style={{ fontSize: "12px", padding: "4px 12px" }}
            onClick={() => setMcpFormOpen(true)}
          >
            + Serveur MCP Custom
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
              className={`locaryn-chip${categoryFilter === "extension" ? " locaryn-chip-on" : ""}`}
              onClick={() => setCategoryFilter("extension")}
            >
              Extensions & MCP
            </button>
            <button
              type="button"
              className={`locaryn-chip${categoryFilter === "connector" ? " locaryn-chip-on" : ""}`}
              onClick={() => setCategoryFilter("connector")}
            >
              Connecteurs Réseau / BDD
            </button>
            <button
              type="button"
              className={`locaryn-chip${categoryFilter === "plugin" ? " locaryn-chip-on" : ""}`}
              onClick={() => setCategoryFilter("plugin")}
            >
              Plugins d'Exécution
            </button>
          </div>

          <div className="locaryn-model-grid">
            {filteredTypes.map((t) => {
              const isEnabled = enabledIds.has(t.type_id);
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
                    <span className="locaryn-tag">{t.category}</span>
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
                        + Ajouter serveur SSH
                      </button>
                    ) : (
                      <button
                        type="button"
                        className={`locaryn-btn-${isEnabled ? "ghost" : "primary"}`}
                        onClick={() => toggleEnable(t.type_id)}
                      >
                        {isEnabled ? "Actif ✓" : "Installer / Activer"}
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
            Extensions & Connecteurs Actifs
          </h3>
          <div className="locaryn-model-grid" style={{ marginBottom: "28px" }}>
            {Array.from(enabledIds).map((id) => {
              const t = types.find((item) => item.type_id === id);
              if (!t) return null;
              return (
                <div key={t.type_id} className="locaryn-box-card">
                  <div className="locaryn-box-head">
                    <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                      <span style={{ fontSize: "24px" }}>{t.icon}</span>
                      <div>
                        <h3 className="locaryn-box-name">{t.display_name}</h3>
                        <span className="locaryn-tag locaryn-tag-installed">actif</span>
                      </div>
                    </div>
                  </div>
                  <p className="locaryn-box-desc">{t.summary}</p>
                  <div
                    style={{
                      marginTop: "auto",
                      paddingTop: "12px",
                      borderTop: "1px solid var(--border)",
                      display: "flex",
                      justifyContent: "flex-end",
                    }}
                  >
                    <button
                      type="button"
                      className="locaryn-btn-ghost"
                      style={{ color: "var(--danger)", fontSize: "12px" }}
                      onClick={() => toggleEnable(t.type_id)}
                    >
                      Désactiver
                    </button>
                  </div>
                </div>
              );
            })}
          </div>

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
          style={{ width: "480px", margin: "100px auto" }}
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

      {extDialogOpen && (
        <ExtensionInstallDialog
          kind="extension"
          onClose={() => setExtDialogOpen(false)}
          // Une extension activée enregistre ses serveurs MCP dans le runtime :
          // re-lire la liste pour qu'ils apparaissent ici, à côté des serveurs
          // ajoutés à la main — et basculer sur l'onglet qui les montre.
          onExtensionInstalled={async () => {
            setTab("installed");
            await refresh();
          }}
        />
      )}
    </div>
  );
}
