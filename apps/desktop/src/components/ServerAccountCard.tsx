import { useState } from "react";
import { MACHINE_ACCOUNT_EVENT, type MachineAccount, core } from "../lib/core";
import { confirmer } from "../lib/dialog";

type Props = {
  account: MachineAccount;
  avatarUrl: string | null;
};

/**
 * L'en-tête du compte quand cette machine utilise un compte du serveur : son
 * nom, son rôle, et le retour au profil local.
 */
export function ServerAccountCard({ account, avatarUrl }: Props) {
  const [busy, setBusy] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);

  async function revenirAuLocal() {
    if (!(await confirmer("Revenir au profil local ? Le compte reste sur le serveur."))) return;
    setBusy(true);
    setErreur(null);
    try {
      await core.setMachineAccount(null);
      window.dispatchEvent(new Event(MACHINE_ACCOUNT_EVENT));
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="locaryn-account-profile-head">
      <div
        className="locaryn-account-avatar"
        role="img"
        aria-label={`Avatar de ${account.username}`}
      >
        {avatarUrl ? <img src={avatarUrl} alt="" /> : account.username.slice(0, 1).toUpperCase()}
      </div>
      <div className="locaryn-account-profile-copy">
        <span className="locaryn-account-eyebrow">COMPTE DU SERVEUR</span>
        <h3>{account.username}</h3>
        <p>
          {account.role === "admin" ? "Administrateur" : "Membre"} du serveur Locaryn de cette
          machine. L'application de ce PC utilise ce compte.
        </p>
        {erreur && <div className="locaryn-vp-error">{erreur}</div>}
      </div>
      <button
        type="button"
        className="locaryn-btn-ghost"
        disabled={busy}
        onClick={() => void revenirAuLocal()}
      >
        Revenir au profil local
      </button>
    </div>
  );
}
