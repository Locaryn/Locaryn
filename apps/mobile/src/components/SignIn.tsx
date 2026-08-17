import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import { type MobileStatus, api } from "../lib/core";

type Props = {
  status: MobileStatus;
  onSignedIn: (s: MobileStatus) => void;
  /** Un serveur vient d'être ajouté : l'écran doit repasser aux identifiants. */
  onRegistered: (s: MobileStatus) => void;
  onScan: () => void;
  /**
   * Les réglages, atteignables sans être connecté.
   *
   * Un téléphone en retard sur son serveur n'arrive plus à se connecter :
   * si la mise à jour n'était accessible qu'après la connexion, il n'y aurait
   * aucune façon d'en sortir depuis l'application.
   */
  onSettings: () => void;
};

/**
 * Signing in on a phone.
 *
 * Deux écrans en un. Tant qu'aucun serveur n'est connu, il n'y a rien à quoi
 * s'identifier : on demande l'adresse, ou le code qui la porte. Une fois le
 * serveur enregistré, l'adresse redevient un détail — elle suit les codes
 * scannés — et il ne reste que l'identifiant et le mot de passe.
 */
export function SignIn({ status, onSignedIn, onRegistered, onScan, onSettings }: Props) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [address, setAddress] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /** Enregistre le serveur depuis l'adresse tapée, puis passe à la connexion. */
  async function addByAddress() {
    setBusy(true);
    setError(null);
    try {
      onRegistered(await api.registerAddress(address));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function submit() {
    if (!username.trim() || !password) {
      setError("Renseignez votre identifiant et votre mot de passe.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const next = await api.signIn(username.trim(), password);
      setPassword("");
      onSignedIn(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  // Première ouverture : aucun serveur n'est connu, donc aucun identifiant ne
  // peut fonctionner. Proposer les deux champs quand même ne mènerait qu'à un
  // échec incompréhensible ; on demande d'abord où joindre le serveur.
  if (status.servers === 0) {
    return (
      <div className="lo-screen">
        <div className="lo-bar">
          <span className="lo-bar-spacer" />
          <button
            type="button"
            className="lo-bar-icon"
            onClick={onSettings}
            aria-label="Réglages"
            title="Version, mise à jour"
          >
            <Icon name="settings" />
          </button>
        </div>
        <div className="lo-center">
          <h1 className="lo-title">Locaryn</h1>
          <p className="lo-sub">Indiquez où joindre votre serveur.</p>

          <div>
            <label className="lo-label" htmlFor="a">
              Adresse du serveur
            </label>
            <input
              id="a"
              className="lo-input"
              autoCapitalize="none"
              autoCorrect="off"
              inputMode="url"
              placeholder="192.168.1.20"
              value={address}
              disabled={busy}
              onChange={(e) => setAddress(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void addByAddress()}
            />
          </div>

          <button type="button" className="lo-btn" disabled={busy} onClick={addByAddress}>
            {busy ? "Connexion…" : "Continuer"}
          </button>

          {error && <p className="lo-error">{error}</p>}

          <p className="lo-hint">
            Le code affiché par l'application de bureau (Réglages → Appareils) fait la même chose en
            une fois, et porte en plus le certificat du serveur — nécessaire pour s'y connecter
            depuis l'extérieur du réseau local.
          </p>

          <button type="button" className="lo-btn-ghost" onClick={onScan}>
            Scanner un QR code
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="lo-screen">
      <div className="lo-bar">
        <span className="lo-bar-spacer" />
        <button
          type="button"
          className="lo-bar-icon"
          onClick={onSettings}
          aria-label="Réglages"
          title="Version, mise à jour"
        >
          <Icon name="settings" />
        </button>
      </div>
      <div className="lo-center">
        <h1 className="lo-title">{status.server_name ?? "Locaryn"}</h1>
        <p className="lo-sub">
          {status.travelling ? "Connexion depuis l'extérieur." : "Connexion sur le réseau local."}
        </p>

        <div>
          <label className="lo-label" htmlFor="u">
            Identifiant
          </label>
          <input
            id="u"
            className="lo-input"
            autoCapitalize="none"
            autoCorrect="off"
            value={username}
            disabled={busy}
            onChange={(e) => setUsername(e.target.value)}
          />
        </div>

        <div>
          <label className="lo-label" htmlFor="p">
            Mot de passe
          </label>
          <input
            id="p"
            className="lo-input"
            type="password"
            value={password}
            disabled={busy}
            onChange={(e) => setPassword(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void submit()}
          />
        </div>

        <button type="button" className="lo-btn" disabled={busy} onClick={submit}>
          {busy ? "Connexion…" : "Se connecter"}
        </button>

        {error && <p className="lo-error">{error}</p>}

        <button type="button" className="lo-btn-ghost" onClick={onScan}>
          Scanner un QR code
        </button>
      </div>
    </div>
  );
}
