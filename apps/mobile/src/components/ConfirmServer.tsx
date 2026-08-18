import { Icon } from "@locaryn/ui-core";
import { useState } from "react";

/** Ce qu'un code d'appairage porte, tel que le téléphone peut le lire sans l'enregistrer. */
export type ProvisioningApercu = {
  serverUrl: string;
  organisation: string;
  accessMode?: string | null;
};

const MODES: Record<string, { label: string; explication: string }> = {
  local: {
    label: "Réseau local",
    explication: "Cette adresse ne vaut que sur le même Wi-Fi que l'ordinateur.",
  },
  public: {
    label: "Port ouvert",
    explication:
      "Un port a été redirigé vers cet ordinateur, ou il a une adresse fixe : joignable depuis n'importe où.",
  },
  tunnel: {
    label: "Tunnel sortant",
    explication:
      "L'adresse passe par un relais et peut expirer. Si la connexion cesse de fonctionner un jour, c'est probablement pour ça — reprendre un nouveau code suffit.",
  },
};

type Props = {
  apercu: ProvisioningApercu;
  busy: boolean;
  success?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
};

/**
 * Ce que le code contient, avant qu'il change quoi que ce soit.
 *
 * Un code scanné modifiait la configuration du téléphone d'un coup : la
 * caméra se refermait, et l'appareil parlait déjà à un autre serveur. Rien ne
 * disait à quel serveur, ni par quel chemin. Cet écran s'interpose : il montre
 * ce qui a été lu, et n'enregistre qu'au geste explicite.
 */
export function ConfirmServer({ apercu, busy, success = false, onConfirm, onCancel }: Props) {
  const [avance, setAvance] = useState(false);
  const mode = apercu.accessMode ? MODES[apercu.accessMode] : null;

  return (
    <div className="lo-screen">
      <div className="lo-center">
        <Icon name="server" size={32} />
        <h1 className="lo-title">Se connecter à ce serveur ?</h1>
        <p className="lo-sub">
          {apercu.organisation || "Un serveur Locaryn"} vient d'être lu depuis le code. Rien n'est
          encore enregistré.
        </p>

        <div className="lo-card" style={{ width: "100%", flexDirection: "column", gap: 4 }}>
          <span className="lo-card-title">{apercu.organisation || "Serveur"}</span>
          {mode && <span className="lo-hint">{mode.label}</span>}
        </div>

        {mode && <p className="lo-hint">{mode.explication}</p>}

        <button
          type="button"
          className="lo-btn-small"
          onClick={() => setAvance((v) => !v)}
          style={{ alignSelf: "flex-start" }}
        >
          {avance ? "Masquer l'adresse" : "Voir l'adresse"}
        </button>
        {avance && (
          <p className="lo-hint" style={{ wordBreak: "break-all" }}>
            {apercu.serverUrl}
          </p>
        )}

        <button type="button" className="lo-btn" disabled={busy || success} onClick={onConfirm}>
          {busy ? "Connexion…" : "Se connecter"}
        </button>
        <button
          type="button"
          className="lo-btn-ghost"
          disabled={busy || success}
          onClick={onCancel}
        >
          Annuler
        </button>
      </div>

      {success && (
        <div className="lo-connection-feedback">
          <div className="lo-success-badge">
            <svg className="lo-checkmark-svg" viewBox="0 0 52 52">
              <circle className="lo-checkmark-circle" cx="26" cy="26" r="24" />
              <path className="lo-checkmark-check" d="M14 27l8 8 16-16" />
            </svg>
            <div style={{ fontWeight: 800, fontSize: 18, color: "var(--text)" }}>
              Serveur enregistré avec succès !
            </div>
            <div style={{ fontSize: 14, color: "var(--text-faint)" }}>
              {apercu.organisation || apercu.serverUrl}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * Lire ce qu'un code contient, sans l'enregistrer.
 *
 * Rend `null` sur tout ce qui n'est pas un code d'appairage reconnaissable —
 * la vérification qui compte reste entièrement du côté du serveur, au moment
 * de l'enregistrement ; ceci ne sert qu'à remplir l'écran de confirmation.
 */
export function lireApercu(brut: string): ProvisioningApercu | null {
  try {
    const v = JSON.parse(brut);
    if (typeof v !== "object" || v === null) return null;
    const serverUrl = v.serverUrl ?? v.server_url;
    if (typeof serverUrl !== "string" || !serverUrl) return null;
    return {
      serverUrl,
      organisation: typeof v.organisation === "string" ? v.organisation : "",
      accessMode:
        typeof v.accessMode === "string"
          ? v.accessMode
          : typeof v.access_mode === "string"
            ? v.access_mode
            : null,
    };
  } catch {
    return null;
  }
}
