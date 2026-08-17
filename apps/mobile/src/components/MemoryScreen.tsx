import { useCallback, useEffect, useState } from "react";
import { type MemoryEntry, api } from "../lib/core";
import { Screen } from "./Screen";

type Props = { onBack: () => void };

/**
 * Ce que le serveur retient de vous.
 *
 * Son propre écran, pas une section coincée entre deux réglages : c'est une
 * liste qui grandit, et elle grandit surtout toute seule — le modèle des
 * petites tâches y dépose ce qu'il comprend de vos habitudes au fil des
 * conversations. On vient ici pour lire ce qu'il a compris, corriger, oublier.
 */
export function MemoryScreen({ onBack }: Props) {
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
    <Screen title="Mémoire" onBack={onBack}>
      <p className="lo-hint">
        Vos préférences, vos habitudes, ce sur quoi vous travaillez. Le modèle s'en sert pour
        répondre — y compris pour écrire un prompt d'image à votre goût. Ce qui est écrit ici a pu
        l'être par vous, ou compris tout seul au fil des conversations ; tout est modifiable.
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
    </Screen>
  );
}
