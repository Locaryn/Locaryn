import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useRef, useState } from "react";
import { type MemorySortStatus, core } from "../lib/core";
import { useSimpleUi } from "../lib/profile";

const POLL_MS = 700;

/**
 * Le tri forcé de la mémoire.
 *
 * Au repos, Locaryn trie tout seul quand le modèle ne sert plus à personne.
 * Ce bouton fait la même chose tout de suite et sans limite : toutes les
 * conversations sont relues, la mémoire est rangée, les phrases d'accueil sont
 * réécrites. Le modèle reste chargé pendant ce temps ; le délai avant son
 * déchargement ne repart qu'à la fin.
 */
export function MemorySortPanel({ onFinished }: { onFinished: () => void }) {
  const simple = useSimpleUi();
  const [status, setStatus] = useState<MemorySortStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number | null>(null);
  const wasRunning = useRef(false);

  const poll = useCallback(async () => {
    try {
      const s = await core.memorySortStatus();
      setStatus(s);
      if (wasRunning.current && !s.running) onFinished();
      wasRunning.current = s.running;
      if (s.running) timer.current = window.setTimeout(poll, POLL_MS);
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    }
  }, [onFinished]);

  // Un tri lancé avant l'ouverture de l'écran se retrouve en cours de route.
  useEffect(() => {
    void poll();
    return () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [poll]);

  async function start() {
    setError(null);
    try {
      await core.forceMemorySort();
      wasRunning.current = true;
      void poll();
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    }
  }

  const running = status?.running === true;
  const pct =
    status && status.tasksTotal > 0
      ? Math.min(100, Math.round((status.tasksDone / status.tasksTotal) * 100))
      : 0;

  return (
    <section className="locaryn-sort" aria-live="polite">
      <div className="locaryn-sort-head">
        <div>
          <h4>{simple ? "Ranger ma mémoire maintenant" : "Trier la mémoire maintenant"}</h4>
          <p>
            {simple
              ? "Locaryn range déjà tout seul quand vous ne l'utilisez pas. Ce bouton le fait tout de suite, sur toutes vos conversations."
              : "Relit tout l'historique, range les fiches et prépare les phrases d'accueil, sans attendre l'inactivité du modèle. Le modèle reste chargé pendant le tri."}
          </p>
        </div>
        <button type="button" className="locaryn-btn-primary" disabled={running} onClick={start}>
          <Icon name={running ? "refresh" : "sparkle"} size={15} />
          {running ? "Tri en cours…" : simple ? "Ranger" : "Trier"}
        </button>
      </div>

      {error && <div className="locaryn-vp-error">{error}</div>}

      {status && (running || status.finished) && (
        <div className="locaryn-sort-body">
          <div
            className="locaryn-sort-bar"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={status.finished ? 100 : pct}
          >
            <span style={{ width: `${status.finished ? 100 : pct}%` }} />
          </div>
          <div className="locaryn-sort-counters">
            <span>
              Conversations analysées{" "}
              <strong>
                {status.conversationsDone} / {status.conversationsTotal}
              </strong>
            </span>
            <span>
              Tâches{" "}
              <strong>
                {status.tasksDone} / {status.tasksTotal}
              </strong>
            </span>
            <span className="locaryn-sort-phase">{status.phase}</span>
          </div>

          {status.recent.length > 0 && (
            <ul className="locaryn-sort-recent">
              {[...new Set(status.recent)].map((titre) => (
                <li key={titre}>
                  <Icon name="check" size={12} /> {titre}
                </li>
              ))}
            </ul>
          )}

          {status.finished && !status.error && (
            <p className="locaryn-sort-done">
              Terminé : {status.factsLearned} fait{status.factsLearned > 1 ? "s" : ""} retenu
              {status.factsLearned > 1 ? "s" : ""}, {status.cardsMerged} fiche
              {status.cardsMerged > 1 ? "s" : ""} réunie{status.cardsMerged > 1 ? "s" : ""},{" "}
              {status.suggestionsWritten} phrase{status.suggestionsWritten > 1 ? "s" : ""} d'accueil
              écrite{status.suggestionsWritten > 1 ? "s" : ""}.
              {status.unloadInSecs !== null && (
                <>
                  {" "}
                  Le modèle se déchargera après {Math.round(status.unloadInSecs / 60)} minutes sans
                  utilisation.
                </>
              )}
            </p>
          )}
          {status.error && <div className="locaryn-vp-error">{status.error}</div>}
        </div>
      )}
    </section>
  );
}
