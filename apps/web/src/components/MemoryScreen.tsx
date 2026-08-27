import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type MemoryEntry, api } from "../lib/core";
import { Screen } from "./Screen";

type Props = { onBack: () => void };

/** Le temps que met un souvenir à s'effacer avant de quitter la liste. */
const FORGET_MS = 320;

const CATEGORIES = [
  { value: "preference", label: "Préférence" },
  { value: "habitude", label: "Habitude" },
  { value: "projet", label: "Projet" },
  { value: "fait", label: "Fait" },
];

function labelFor(category: string): string {
  return CATEGORIES.find((c) => c.value === category.trim().toLowerCase())?.label ?? category;
}

function learnedOn(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "date inconnue";
  return `appris le ${d.toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" })}`;
}

/**
 * Ce que le serveur retient de vous — un texte, pas une liste.
 *
 * Le même écran que sur le téléphone : un paragraphe par souvenir, aucune
 * délimitation au repos. Le clic ouvre le souvenir, révèle sa métadonnée et
 * propose de l'oublier — la souris comme le pouce, un seul geste.
 */
export function MemoryScreen({ onBack }: Props) {
  const [entries, setEntries] = useState<MemoryEntry[] | null>(null);
  const [draft, setDraft] = useState("");
  const [category, setCategory] = useState("preference");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Le souvenir ouvert : sur un téléphone, la touche remplace le survol. */
  const [ouvert, setOuvert] = useState<string | null>(null);
  const [forgetting, setForgetting] = useState<string | null>(null);

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
    setForgetting(id);
    await new Promise((resolve) => setTimeout(resolve, FORGET_MS));
    try {
      await api.forget(id);
      setEntries((prev) => prev?.filter((e) => e.id !== id) ?? null);
      setOuvert(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setForgetting(null);
    }
  }

  return (
    <Screen title="Mémoire" onBack={onBack}>
      <p className="lo-hint">
        Vos préférences, vos habitudes, ce sur quoi vous travaillez — ce que le modèle emporte avec
        vos messages. Cliquez un passage pour voir d'où il vient et l'oublier.
      </p>

      {entries === null && !error && <p className="lo-sub">Chargement…</p>}
      {entries?.length === 0 && <p className="lo-sub">Rien pour l'instant.</p>}

      {entries && entries.length > 0 && (
        <article className="lo-memory-document">
          {entries.map((e) => {
            const actif = ouvert === e.id;
            return (
              <div
                key={e.id}
                className={`lo-memory-para${actif ? " is-open" : ""}${forgetting === e.id ? " is-forgetting" : ""}`}
              >
                <button
                  type="button"
                  className="lo-memory-text"
                  aria-expanded={actif}
                  onClick={() => setOuvert(actif ? null : e.id)}
                >
                  {e.content}
                </button>
                {actif && (
                  <div className="lo-memory-foot">
                    <span className="lo-memory-meta">
                      {labelFor(e.category)}
                      {e.created_at ? ` · ${learnedOn(e.created_at)}` : ""}
                    </span>
                    <button
                      type="button"
                      className="lo-btn-small"
                      onClick={() => void forget(e.id)}
                      aria-label={`Oublier : ${e.content}`}
                    >
                      <Icon name="trash" size={15} /> Oublier
                    </button>
                  </div>
                )}
              </div>
            );
          })}
        </article>
      )}

      <label className="lo-label" htmlFor="mem-cat">
        Catégorie
      </label>
      <select
        id="mem-cat"
        className="lo-input"
        value={category}
        onChange={(e) => setCategory(e.target.value)}
      >
        {CATEGORIES.map((c) => (
          <option key={c.value} value={c.value}>
            {c.label}
          </option>
        ))}
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
