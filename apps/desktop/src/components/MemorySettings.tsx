import { useCallback, useEffect, useState } from "react";
import { type MemoryEntry, core } from "../lib/core";
import { taskCenter } from "../lib/taskCenter";

/**
 * La mémoire est un texte, pas une liste.
 *
 * Un paragraphe par souvenir, écrit par l'assistant. Au repos rien ne
 * délimite quoi que ce soit : c'est un document qu'on lit, pas un formulaire
 * qu'on remplit. Le survol révèle les limites du souvenir survolé et sa
 * métadonnée ; le clic droit l'oublie.
 *
 * Les entrées restent séparées dans la base — sans ça on ne pourrait pas en
 * oublier une seule proprement.
 */

/** Le temps que met un souvenir à s'effacer avant de quitter la liste. */
const FORGET_MS = 320;

const CATEGORY_LABELS: Record<string, string> = {
  preference: "Préférence",
  préférence: "Préférence",
  habit: "Habitude",
  habitude: "Habitude",
  project: "Projet",
  projet: "Projet",
  fact: "Information personnelle",
  fait: "Information personnelle",
};

function labelFor(category: string): string {
  const key = category.trim().toLowerCase();
  return CATEGORY_LABELS[key] ?? (category.trim() || "Autre");
}

/** La date d'apprentissage, écrite comme on la dirait. */
function learnedOn(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "date inconnue";
  return `appris le ${d.toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" })}`;
}

export function MemorySettings() {
  const [entries, setEntries] = useState<MemoryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** Le souvenir en train de s'effacer : il reste en place le temps du fondu. */
  const [forgetting, setForgetting] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setEntries(await core.listMemory());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  /** Oublier un souvenir : d'abord il s'efface, ensuite il quitte la liste. */
  const forget = useCallback(async (entry: MemoryEntry) => {
    setForgetting(entry.id);
    await new Promise((resolve) => setTimeout(resolve, FORGET_MS));
    try {
      await core.forgetMemory(entry.id);
      setEntries((prev) => prev.filter((e) => e.id !== entry.id));
      const id = taskCenter.add({ type: "edit", label: "Souvenir oublié" });
      taskCenter.done(id, { detail: entry.content.slice(0, 60) });
    } catch (e) {
      setError(String(e));
    } finally {
      setForgetting(null);
    }
  }, []);

  async function forgetAll() {
    setBusy(true);
    try {
      const count = await core.forgetAllMemory();
      setEntries([]);
      const id = taskCenter.add({ type: "edit", label: "Mémoire vidée" });
      taskCenter.done(id, { detail: `${count} souvenir${count === 1 ? "" : "s"}` });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="locaryn-memory-settings">
      <div className="locaryn-memory-intro">
        <div>
          <span className="locaryn-account-eyebrow">MÉMOIRE DU COMPTE</span>
          <h3>Ce que Locaryn retient de vous</h3>
          <p>
            Tout ce qui accompagne vos messages, écrit d'un seul tenant. Survolez un passage pour
            voir où commence et où finit le souvenir ; clic droit pour l'oublier.
          </p>
        </div>
        <span className="locaryn-memory-count">
          {entries.length} souvenir{entries.length === 1 ? "" : "s"}
        </span>
      </div>

      {error && <div className="locaryn-vp-error">{error}</div>}

      {entries.length === 0 ? (
        <div className="locaryn-memory-empty">
          <strong>La mémoire est vide.</strong>
          <span>Locaryn ne conserve encore aucune information durable sur vous.</span>
        </div>
      ) : (
        <>
          <article className="locaryn-memory-document" aria-label="Ce que Locaryn retient">
            {entries.map((entry) => (
              <p
                key={entry.id}
                className={`locaryn-memory-para${forgetting === entry.id ? " is-forgetting" : ""}`}
                title="Clic droit pour oublier ce souvenir"
                onContextMenu={(e) => {
                  e.preventDefault();
                  if (!forgetting) void forget(entry);
                }}
              >
                {entry.content}
                <span className="locaryn-memory-meta" aria-hidden="true">
                  {labelFor(entry.category)} · {learnedOn(entry.created_at)}
                </span>
              </p>
            ))}
          </article>

          <div className="locaryn-memory-actions">
            <span>
              Pour corriger une information plutôt que l'oublier, demandez-le à Locaryn dans le
              chat.
            </span>
            <button
              type="button"
              className="locaryn-btn-ghost"
              disabled={busy}
              onClick={() => void forgetAll()}
            >
              Tout oublier
            </button>
          </div>
        </>
      )}
    </div>
  );
}
