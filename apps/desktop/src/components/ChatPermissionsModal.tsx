import { Icon } from "@locaryn/ui-core";
import { type TrustLevel } from "../lib/core";
import { ModalShell } from "./ModalShell";

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
  if (!isOpen) return null;

  return (
    <ModalShell
      onClose={onClose}
      label="Autorisations et gouvernance du chat"
      className="locaryn-card locaryn-chat-permissions-modal"
    >
      <div className="locaryn-field-head" style={{ marginBottom: "16px" }}>
        <h3 className="locaryn-modal-title">
          <Icon name="shield" size={15} /> Autorisations & Gouvernance du Chat
        </h3>
        <button
          type="button"
          className="locaryn-icon-btn locaryn-modal-close"
          onClick={onClose}
          aria-label="Fermer les autorisations du chat"
        >
          <Icon name="close" size={16} />
        </button>
      </div>

      {/* Section: Trust Level */}
      <div className="locaryn-field" style={{ marginBottom: "24px" }}>
        <label htmlFor="perm-trust" className="locaryn-field-label">
          Niveau de Confiance du Projet / Chat
        </label>
        <select
          id="perm-trust"
          className="locaryn-select"
          value={trustLevel}
          onChange={(e) => onTrustLevelChange?.(e.target.value as TrustLevel)}
        >
          <option value="untrusted">Untrusted (Demander confirmation pour chaque écriture)</option>
          <option value="trusted">Trusted (Auto-approbation des lectures et modifications)</option>
          <option value="sandbox">Sandbox (Lecture seule strict - aucun terminal)</option>
        </select>
        <p className="locaryn-field-hint">
          Définit l'autonomie accordée à l'agent IA pour exécuter des commandes et modifier votre
          code.
        </p>
      </div>

      <div
        className="locaryn-field-actions"
        style={{ marginTop: "24px", display: "flex", justifyContent: "flex-end" }}
      >
        <button type="button" className="locaryn-btn-primary" onClick={onClose}>
          Fermer
        </button>
      </div>
    </ModalShell>
  );
}
