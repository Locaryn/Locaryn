import { useEffect, useState } from "react";
import { core, type SshAiAccess, type SshServer, type TrustLevel } from "../lib/core";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  trustLevel?: TrustLevel;
  onTrustLevelChange?: (level: TrustLevel) => void;
};

export function ChatPermissionsModal({
  isOpen,
  onClose,
  trustLevel = "untrusted",
  onTrustLevelChange,
}: Props) {
  const [sshServers, setSshServers] = useState<SshServer[]>([]);

  useEffect(() => {
    if (isOpen) {
      core.listSshServers().then(setSshServers).catch(() => setSshServers([]));
    }
  }, [isOpen]);

  if (!isOpen) return null;

  async function handleSshAccessChange(id: string, level: SshAiAccess) {
    await core.setSshAiAccess(id, level);
    const updated = await core.listSshServers();
    setSshServers(updated);
  }

  return (
    <div className="locaryn-settings-backdrop" onClick={onClose}>
      <div
        className="locaryn-card"
        style={{
          width: "560px",
          maxHeight: "85vh",
          overflowY: "auto",
          margin: "60px auto",
          border: "1px solid var(--border-strong)",
          boxShadow: "0 12px 32px rgba(0,0,0,0.6)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="locaryn-field-head" style={{ marginBottom: "16px" }}>
          <h3 style={{ margin: 0 }}>🛡️ Autorisations & Gouvernance du Chat</h3>
          <button type="button" className="locaryn-icon-btn" onClick={onClose}>
            ✕
          </button>
        </div>

        {/* Section 1: Trust Level */}
        <div className="locaryn-field" style={{ marginBottom: "24px" }}>
          <label className="locaryn-field-label">Niveau de Confiance du Projet / Chat</label>
          <select
            className="locaryn-select"
            value={trustLevel}
            onChange={(e) => onTrustLevelChange?.(e.target.value as TrustLevel)}
          >
            <option value="untrusted">🛡️ Untrusted (Demander confirmation pour chaque écriture)</option>
            <option value="trusted">⚡ Trusted (Auto-approbation des lectures et modifications)</option>
            <option value="sandbox">🔒 Sandbox (Lecture seule strict - aucun terminal)</option>
          </select>
          <p className="locaryn-field-hint">
            Définit l'autonomie accordée à l'agent IA pour exécuter des commandes et modifier votre code.
          </p>
        </div>

        {/* Section 2: Connector AI Access Gating */}
        <div className="locaryn-field">
          <label className="locaryn-field-label">Autorisations des Connecteurs Actifs (SSH & Extensions)</label>
          <p className="locaryn-field-hint">
            Définissez si l'agent IA peut accéder à vos serveurs distants configurés et quel est son niveau d'autonomie.
          </p>

          {sshServers.length === 0 ? (
            <div className="locaryn-field-hint" style={{ fontStyle: "italic", marginTop: "8px" }}>
              Aucun serveur SSH configuré dans le Store Connecteurs.
            </div>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: "8px", marginTop: "12px" }}>
              {sshServers.map((server) => (
                <div key={server.id} className="locaryn-box-variant-row">
                  <div>
                    <span style={{ fontWeight: 700, fontSize: "var(--text-sm)" }}>{server.name}</span>
                    <span style={{ fontSize: "var(--text-xs)", color: "var(--text-faint)", marginLeft: "8px" }}>
                      {server.username}@{server.host}
                    </span>
                  </div>
                  <select
                    className="locaryn-select locaryn-select-sm"
                    value={server.ai_access}
                    onChange={(e) => handleSshAccessChange(server.id, e.target.value as SshAiAccess)}
                  >
                    <option value="none">Invisible</option>
                    <option value="read_only">Lecture seule</option>
                    <option value="approval">Avec confirmation</option>
                    <option value="trusted">Confiance totale</option>
                  </select>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="locaryn-field-actions" style={{ marginTop: "24px", display: "flex", justifyContent: "flex-end" }}>
          <button type="button" className="locaryn-btn-primary" onClick={onClose}>
            Fermer
          </button>
        </div>
      </div>
    </div>
  );
}
