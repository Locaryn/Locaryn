import { useCallback, useEffect, useState } from "react";
import { type MemoryEntry, type MobileStatus, api } from "../lib/core";
import { Screen } from "./Screen";
import { UpdateButton } from "./UpdateButton";

type Props = {
  status: MobileStatus;
  onBack: () => void;
  onSignedOut: (s: MobileStatus) => void;
};

/**
 * Réglages.
 *
 * Trois choses seulement, parce que le téléphone n'en décide que trois : à
 * quel serveur il parle, ce que ce serveur retient de son utilisateur, et
 * comment se mettre à jour. Tout le reste — modèles, extensions, comptes — est
 * une décision de la machine à l'autre bout, et se prend là-bas.
 */
export function Settings({ status, onBack, onSignedOut }: Props) {
  return (
    <Screen title="Réglages" onBack={onBack} action={<UpdateButton />}>
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

      <Memory />
    </Screen>
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
