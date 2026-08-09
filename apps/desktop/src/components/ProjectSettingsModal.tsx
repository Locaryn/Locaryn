import { useState } from "react";
import type { Project, TrustLevel } from "../lib/core";
import { ModalShell } from "./ModalShell";

type Props = {
  project: Project | null;
  isOpen: boolean;
  onClose: () => void;
  onSave?: (updated: Project) => void;
};

const DEFAULT_PROTECTED_FILES = ".env\n.env.*\n*.pem\n*.key\nid_rsa\ncredentials.json\nsecrets/*";

export function ProjectSettingsModal({ project, isOpen, onClose, onSave }: Props) {
  const [trustLevel, setTrustLevel] = useState<TrustLevel>(project?.trust_level ?? "untrusted");
  const [protectedFiles, setProtectedFiles] = useState(DEFAULT_PROTECTED_FILES);
  const [requireWriteApproval, setRequireWriteApproval] = useState(true);
  const [requireShellApproval, setRequireShellApproval] = useState(true);
  const [allowedConnectors, setAllowedConnectors] = useState<Record<string, boolean>>({
    ssh_server_1: true,
    postgres_mcp: false,
    brave_web_search: true,
    docker_engine: false,
    lsp_plugin: true,
  });

  const [saved, setSaved] = useState(false);

  if (!isOpen || !project) return null;

  function toggleConnector(id: string) {
    setAllowedConnectors((prev) => ({ ...prev, [id]: !prev[id] }));
  }

  function handleSave() {
    if (project) {
      const updated: Project = {
        ...project,
        trust_level: trustLevel,
      };
      onSave?.(updated);
    }
    setSaved(true);
    setTimeout(() => {
      setSaved(false);
      onClose();
    }, 1000);
  }

  return (
    <ModalShell
      onClose={onClose}
      label="Paramètres du projet"
      style={{
        width: "650px",
        maxHeight: "85vh",
        overflowY: "auto",
        margin: "50px auto",
        border: "1px solid var(--border-strong)",
        boxShadow: "0 16px 40px rgba(0,0,0,0.7)",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: "16px",
        }}
      >
        <div>
          <h3 style={{ margin: 0 }}>⚙️ Paramètres du Projet : {project.name}</h3>
          <span style={{ fontSize: "var(--text-xs)", color: "var(--text-faint)" }}>
            {project.path}
          </span>
        </div>
        <button type="button" className="locaryn-icon-btn" onClick={onClose}>
          ✕
        </button>
      </div>

      {/* 1. Trust Level */}
      <div className="locaryn-field" style={{ marginBottom: "20px" }}>
        <label htmlFor="projset-trust" className="locaryn-field-label">
          Niveau de Confiance Global du Projet
        </label>
        <select
          id="projset-trust"
          className="locaryn-select"
          value={trustLevel}
          onChange={(e) => setTrustLevel(e.target.value as TrustLevel)}
        >
          <option value="untrusted">
            🛡️ Untrusted (Recommandé : Demander confirmation pour chaque écriture)
          </option>
          <option value="trusted">
            ⚡ Trusted (Auto-approbation des lectures et modifications de code)
          </option>
          <option value="sandbox">
            🔒 Sandbox (Lecture seule stricte - aucun terminal ni modification)
          </option>
        </select>
      </div>

      {/* 2. Protected & Excluded Files */}
      <div className="locaryn-field" style={{ marginBottom: "20px" }}>
        <label htmlFor="projset-protected" className="locaryn-field-label">
          🛡️ Fichiers Protégés & Interdits à l'IA (Patterns Glob)
        </label>
        <textarea
          id="projset-protected"
          className="locaryn-input"
          rows={4}
          value={protectedFiles}
          onChange={(e) => setProtectedFiles(e.target.value)}
          style={{ fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}
          placeholder=".env&#10;*.pem&#10;secrets/*"
        />
        <p className="locaryn-field-hint">
          L'agent IA aura une interdiction absolue d'accéder, de lire ou de modifier les fichiers
          correspondant à ces motifs.
        </p>
      </div>

      {/* 3. Granular Confirmations */}
      <div className="locaryn-field" style={{ marginBottom: "20px" }}>
        {/* Titre d'un groupe de cases à cocher, pas l'étiquette d'un champ :
            chaque case porte déjà la sienne. */}
        <div className="locaryn-field-label">🔔 Exigences de Confirmation par Action</div>
        <div style={{ display: "flex", flexDirection: "column", gap: "8px", marginTop: "6px" }}>
          <label className="locaryn-checkbox-row">
            <input
              type="checkbox"
              checked={requireWriteApproval}
              onChange={(e) => setRequireWriteApproval(e.target.checked)}
            />
            <span>
              Toujours demander la confirmation de l'utilisateur avant d'écrire ou modifier un
              fichier
            </span>
          </label>
          <label className="locaryn-checkbox-row">
            <input
              type="checkbox"
              checked={requireShellApproval}
              onChange={(e) => setRequireShellApproval(e.target.checked)}
            />
            <span>
              Toujours demander la confirmation de l'utilisateur avant d'exécuter une commande
              terminal
            </span>
          </label>
        </div>
      </div>

      {/* 4. Connectors Allowed for this Project */}
      <div className="locaryn-field">
        {/* Idem : coiffe la liste des connecteurs, ne désigne aucun champ. */}
        <div className="locaryn-field-label">
          🔌 Extensions & Connecteurs Autorisés pour ce Projet
        </div>
        <p className="locaryn-field-hint" style={{ marginBottom: "8px" }}>
          Cochez les connecteurs auxquels ce projet a le droit d'accéder durant les sessions de
          chat.
        </p>

        <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
          <label
            className="locaryn-checkbox-row"
            style={{
              background: "var(--bg)",
              padding: "8px 12px",
              borderRadius: "var(--radius-xs)",
              border: "1px solid var(--border)",
            }}
          >
            <input
              type="checkbox"
              checked={allowedConnectors.ssh_server_1 ?? true}
              onChange={() => toggleEnable("ssh_server_1")}
            />
            <div>
              <span style={{ fontWeight: 700 }}>🖧 Serveur SSH Distant</span>
              <span
                style={{
                  fontSize: "var(--text-xs)",
                  color: "var(--text-faint)",
                  display: "block",
                }}
              >
                Permet à l'agent d'interagir avec votre serveur SSH lié.
              </span>
            </div>
          </label>

          <label
            className="locaryn-checkbox-row"
            style={{
              background: "var(--bg)",
              padding: "8px 12px",
              borderRadius: "var(--radius-xs)",
              border: "1px solid var(--border)",
            }}
          >
            <input
              type="checkbox"
              checked={allowedConnectors.brave_web_search ?? true}
              onChange={() => toggleEnable("brave_web_search")}
            />
            <div>
              <span style={{ fontWeight: 700 }}>🔍 Recherche Web Brave & Scraper</span>
              <span
                style={{
                  fontSize: "var(--text-xs)",
                  color: "var(--text-faint)",
                  display: "block",
                }}
              >
                Autorise l'agent à effectuer des recherches en ligne pour la documentation du
                projet.
              </span>
            </div>
          </label>

          <label
            className="locaryn-checkbox-row"
            style={{
              background: "var(--bg)",
              padding: "8px 12px",
              borderRadius: "var(--radius-xs)",
              border: "1px solid var(--border)",
            }}
          >
            <input
              type="checkbox"
              checked={allowedConnectors.postgres_mcp ?? false}
              onChange={() => toggleEnable("postgres_mcp")}
            />
            <div>
              <span style={{ fontWeight: 700 }}>🐘 Base de Données PostgreSQL / MySQL</span>
              <span
                style={{
                  fontSize: "var(--text-xs)",
                  color: "var(--text-faint)",
                  display: "block",
                }}
              >
                Permet d'inspecter les tables et schémas SQL pour ce projet.
              </span>
            </div>
          </label>

          <label
            className="locaryn-checkbox-row"
            style={{
              background: "var(--bg)",
              padding: "8px 12px",
              borderRadius: "var(--radius-xs)",
              border: "1px solid var(--border)",
            }}
          >
            <input
              type="checkbox"
              checked={allowedConnectors.lsp_plugin ?? true}
              onChange={() => toggleEnable("lsp_plugin")}
            />
            <div>
              <span style={{ fontWeight: 700 }}>⚡ Language Server Protocol (LSP)</span>
              <span
                style={{
                  fontSize: "var(--text-xs)",
                  color: "var(--text-faint)",
                  display: "block",
                }}
              >
                Analyse de type précise et autocomplétion pour le code du projet.
              </span>
            </div>
          </label>
        </div>
      </div>

      <div
        className="locaryn-field-actions"
        style={{ marginTop: "24px", display: "flex", justifyContent: "flex-end", gap: "8px" }}
      >
        <button type="button" className="locaryn-btn-ghost" onClick={onClose}>
          Annuler
        </button>
        <button type="button" className="locaryn-btn-primary" onClick={handleSave}>
          {saved ? "Enregistré ✓" : "Enregistrer les paramètres du projet"}
        </button>
      </div>
    </ModalShell>
  );

  function toggleEnable(key: string) {
    setAllowedConnectors((prev) => ({ ...prev, [key]: !prev[key] }));
  }
}
