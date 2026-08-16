import { useCallback, useEffect, useState } from "react";
import {
  CATALOGUE,
  type MemoryEntry,
  type MobileStatus,
  type PhoneExtension,
  api,
} from "../lib/core";
import { UpdateButton } from "./UpdateButton";

type Props = {
  status: MobileStatus;
  onBack: () => void;
  onSignedOut: (s: MobileStatus) => void;
  /** Une extension a bougé : le Studio doit être recalculé. */
  onExtensionsChanged: () => void;
};

/**
 * Réglages.
 *
 * Trois choses seulement, parce que le téléphone n'en décide que trois : à
 * quel serveur il parle, ce que ce serveur retient de son utilisateur, et
 * comment se mettre à jour. Tout le reste — modèles, extensions, comptes — est
 * une décision de la machine à l'autre bout, et se prend là-bas.
 */
export function Settings({ status, onBack, onSignedOut, onExtensionsChanged }: Props) {
  return (
    <div className="lo-screen">
      <div className="lo-bar">
        <button type="button" className="lo-back" onClick={onBack}>
          ← Chat
        </button>
        <span>Réglages</span>
        <UpdateButton />
      </div>

      <div className="lo-studio">
        <section className="lo-section">
          <h2 className="lo-section-title">Serveur</h2>
          <p className="lo-hint">
            {status.server_name ?? "Aucun"}
            {status.travelling ? " — joint depuis l'extérieur" : ""}
          </p>
          <button
            type="button"
            className="lo-btn-ghost"
            onClick={() => void api.signOut().then(onSignedOut)}
          >
            Se déconnecter
          </button>
        </section>

        <Extensions onChanged={onExtensionsChanged} />
        <Memory />
      </div>
    </div>
  );
}

/**
 * Les extensions du serveur, pilotées depuis le téléphone.
 *
 * L'installation se fait sur la machine d'en face : c'est elle qui télécharge
 * le dépôt. Le téléphone ne fait que désigner lequel — il n'a aucun fichier à
 * fournir, et n'a pas à en avoir.
 */
function Extensions({ onChanged }: { onChanged: () => void }) {
  const [installed, setInstalled] = useState<PhoneExtension[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setInstalled(await api.listExtensions());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function act(key: string, run: () => Promise<unknown>) {
    setBusy(key);
    setError(null);
    try {
      await run();
      await reload();
      onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  const byName = new Map((installed ?? []).map((e) => [e.name, e]));

  return (
    <section className="lo-section">
      <h2 className="lo-section-title">Extensions</h2>
      <p className="lo-hint">
        Elles s'installent sur le serveur et valent pour tous ses appareils. Ce qu'elles apportent
        apparaît dans « Créer ».
      </p>

      {installed === null && !error && <p className="lo-sub">Chargement…</p>}

      <ul className="lo-list">
        {CATALOGUE.map((c) => {
          const name = c.repo.split("/")[1];
          const on = byName.get(name);
          const working = busy === c.repo;
          return (
            <li key={c.repo} className="lo-list-item">
              <span className="lo-list-text">
                {c.label}
                <span className="lo-hint"> — {c.note}</span>
              </span>
              {on ? (
                <>
                  <button
                    type="button"
                    className="lo-msg-copy"
                    disabled={working}
                    onClick={() => act(c.repo, () => api.setExtensionEnabled(name, !on.enabled))}
                  >
                    {on.enabled ? "Désactiver" : "Activer"}
                  </button>
                  <button
                    type="button"
                    className="lo-msg-copy"
                    disabled={working}
                    onClick={() => act(c.repo, () => api.removeExtension(name))}
                  >
                    Retirer
                  </button>
                </>
              ) : (
                <button
                  type="button"
                  className="lo-msg-copy"
                  disabled={working}
                  onClick={() => act(c.repo, () => api.installExtension(c.repo))}
                >
                  {working ? "Installation…" : "Installer"}
                </button>
              )}
            </li>
          );
        })}
      </ul>

      {error && <p className="lo-error">{error}</p>}
    </section>
  );
}

/** Ce que le serveur retient de vous, modifiable ici. */
function Memory() {
  const [entries, setEntries] = useState<MemoryEntry[] | null>(null);
  const [draft, setDraft] = useState("");
  const [category, setCategory] = useState("preference");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setEntries(await api.listMemory());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function add() {
    const content = draft.trim();
    if (!content || busy) return;
    setBusy(true);
    try {
      await api.remember(category, content);
      setDraft("");
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function forget(id: string) {
    try {
      await api.forget(id);
      await reload();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section className="lo-section">
      <h2 className="lo-section-title">Ce que le serveur retient</h2>
      <p className="lo-hint">
        Vos préférences, vos habitudes, ce sur quoi vous travaillez. Le modèle s'en sert pour
        répondre — y compris pour écrire un prompt d'image à votre goût.
      </p>

      {entries === null && !error && <p className="lo-sub">Chargement…</p>}
      {entries?.length === 0 && <p className="lo-sub">Rien pour l'instant.</p>}

      <ul className="lo-list">
        {entries?.map((e) => (
          <li key={e.id} className="lo-list-item">
            <span className="lo-tag">{e.category}</span>
            <span className="lo-list-text">{e.content}</span>
            <button
              type="button"
              className="lo-msg-copy"
              onClick={() => void forget(e.id)}
              aria-label={`Oublier : ${e.content}`}
            >
              Oublier
            </button>
          </li>
        ))}
      </ul>

      <label className="lo-label" htmlFor="mem-cat">
        Catégorie
      </label>
      <select
        id="mem-cat"
        className="lo-input"
        value={category}
        onChange={(e) => setCategory(e.target.value)}
      >
        <option value="preference">Préférence</option>
        <option value="habitude">Habitude</option>
        <option value="projet">Projet</option>
        <option value="fait">Fait</option>
      </select>

      <label className="lo-label" htmlFor="mem-new">
        À retenir
      </label>
      <input
        id="mem-new"
        className="lo-input"
        placeholder="Je préfère les réponses courtes."
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && void add()}
      />
      <button type="button" className="lo-btn" disabled={busy || !draft.trim()} onClick={add}>
        {busy ? "Enregistrement…" : "Retenir"}
      </button>

      {error && <p className="lo-error">{error}</p>}
    </section>
  );
}
