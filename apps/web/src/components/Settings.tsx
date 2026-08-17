import { useCallback, useEffect, useState } from "react";
import { type WebStatus, api } from "../lib/core";
import { Screen } from "./Screen";

type Props = {
  status: WebStatus;
  onBack: () => void;
  onSignedOut: (s: WebStatus) => void;
  onMemory: () => void;
};

/**
 * Réglages — le même écran que sur le téléphone : le serveur, ce qu'il
 * retient de vous, et votre profil (identifiant, mot de passe). Le reste —
 * modèles, extensions — se décide sur la machine d'en face, et se regarde
 * depuis le menu principal.
 */
export function Settings({ status, onBack, onSignedOut, onMemory }: Props) {
  const [me, setMe] = useState<{ username: string; role: string; local?: boolean } | null>(null);
  const [current, setCurrent] = useState("");
  const [nouveau, setNouveau] = useState("");
  const [confirme, setConfirme] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  const reload = useCallback(async () => {
    try {
      setMe(await api.me());
    } catch {
      // Le mode local (sans compte) n'a pas de profil à montrer.
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function changePassword() {
    if (busy) return;
    setError(null);
    setDone(false);
    if (nouveau.length < 8) {
      setError("Le nouveau mot de passe doit faire 8 caractères au minimum.");
      return;
    }
    if (nouveau !== confirme) {
      setError("La confirmation ne correspond pas au nouveau mot de passe.");
      return;
    }
    setBusy(true);
    try {
      await api.changePassword(current, nouveau);
      setCurrent("");
      setNouveau("");
      setConfirme("");
      setDone(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Screen title="Réglages" onBack={onBack}>
      {status.signed_in && me && (
        <section className="lo-section">
          <h2 className="lo-section-title">Profil</h2>
          <p className="lo-hint">
            {me.username}
            {me.local
              ? " — compte local"
              : ` — ${me.role === "admin" ? "administrateur" : "membre"}`}
          </p>
          {!me.local && (
            <div className="lo-stack">
              <label className="lo-label" htmlFor="pw-current">
                Mot de passe actuel
              </label>
              <input
                id="pw-current"
                type="password"
                className="lo-input"
                value={current}
                onChange={(e) => setCurrent(e.target.value)}
              />
              <label className="lo-label" htmlFor="pw-nouveau">
                Nouveau mot de passe
              </label>
              <input
                id="pw-nouveau"
                type="password"
                className="lo-input"
                value={nouveau}
                onChange={(e) => setNouveau(e.target.value)}
              />
              <label className="lo-label" htmlFor="pw-confirme">
                Confirmation
              </label>
              <input
                id="pw-confirme"
                type="password"
                className="lo-input"
                value={confirme}
                onChange={(e) => setConfirme(e.target.value)}
              />
              <button
                type="button"
                className="lo-btn"
                disabled={busy || !current || !nouveau || !confirme}
                onClick={() => void changePassword()}
              >
                {busy ? "Enregistrement…" : "Changer le mot de passe"}
              </button>
              {done && <p className="lo-sub">Mot de passe changé.</p>}
              {error && <p className="lo-error">{error}</p>}
            </div>
          )}
        </section>
      )}

      <section className="lo-section">
        <h2 className="lo-section-title">Serveur</h2>
        <p className="lo-hint">{status.server_name ?? "Aucun serveur enregistré"}</p>
      </section>

      {/* La déconnexion vit ici, pas dans la barre du chat : un bouton qui ne
          sert qu'à quitter sa session n'a rien à faire à côté de la saisie. */}
      <section className="lo-section">
        <h2 className="lo-section-title">Session</h2>
        <button
          type="button"
          className="lo-btn-ghost"
          onClick={() => void api.signOut().then(onSignedOut)}
        >
          Se déconnecter
        </button>
      </section>

      {status.signed_in && (
        <section className="lo-section">
          <h2 className="lo-section-title">Personnalisation</h2>
          <button type="button" className="lo-row-nav" onClick={onMemory}>
            <span className="lo-row-text">
              <span className="lo-row-label">Mémoire</span>
              <span className="lo-hint">Ce que le serveur retient de vous</span>
            </span>
            <span className="lo-row-go">›</span>
          </button>
        </section>
      )}
    </Screen>
  );
}
