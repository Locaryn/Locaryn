import { Icon } from "@locaryn/ui-core";
import { useState } from "react";

type Props = {
  busy: boolean;
  error: string | null;
  onScan: () => void;
  onAddress: (address: string) => void;
  onDismiss: () => void;
};

/**
 * Le serveur ne répond plus : proposer de le retrouver, pas de repartir de zéro.
 *
 * Une box qui redémarre change d'adresse ; un tunnel expire. Dans les deux cas
 * c'est le **même** serveur, avec le même compte et les mêmes conversations —
 * seule l'adresse a changé. Le compte reste connecté et rien de l'historique
 * n'est perdu : `reconnectActiveServer` ne touche que l'adresse, jamais
 * l'autorité ni la session.
 */
export function ReconnectPrompt({ busy, error, onScan, onAddress, onDismiss }: Props) {
  const [adresse, setAdresse] = useState("");

  return (
    <div className="lo-scan-overlay" style={{ background: "rgba(0,0,0,0.75)" }}>
      <div className="lo-center" style={{ width: "100%" }}>
        <Icon name="warning" size={28} />
        <h1 className="lo-title">Le serveur ne répond plus</h1>
        <p className="lo-sub">
          Rien n'est perdu : le compte et les conversations restent, il ne manque qu'un chemin pour
          rejoindre le serveur.
        </p>

        <button type="button" className="lo-btn" disabled={busy} onClick={onScan}>
          <Icon name="server" size={16} /> Scanner un nouveau code
        </button>

        <input
          className="lo-input"
          placeholder="Nouvelle adresse si elle a changé (ex. 192.168.1.20)"
          value={adresse}
          onChange={(e) => setAdresse(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && onAddress(adresse.trim())}
        />
        {/*
          Le champ peut rester vide : le serveur a peut-être simplement
          redémarré à la même adresse, et le bouton retente alors celle qu'on
          connaît déjà — pas besoin de la retrouver de mémoire.
        */}
        <button
          type="button"
          className="lo-btn-ghost"
          disabled={busy}
          onClick={() => onAddress(adresse.trim())}
        >
          {busy ? "Connexion…" : "Reprendre à cette adresse"}
        </button>

        {error && <p className="lo-error">{error}</p>}

        <button type="button" className="lo-btn-small" onClick={onDismiss}>
          Plus tard
        </button>
      </div>
    </div>
  );
}
