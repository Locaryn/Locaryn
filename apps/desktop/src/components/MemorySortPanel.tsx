import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useRef, useState } from "react";
import { type CloudModel, type CloudProvider, type MemorySortStatus, core } from "../lib/core";
import { useSimpleUi } from "../lib/profile";

const POLL_MS = 700;

/** Le modèle distant choisi pour un tri unique. */
interface ChoixDistant {
  provider: string;
  model: string;
}

function messageDe(e: unknown): string {
  return String(e).replace(/^Error:\s*/, "");
}

/** Un modèle qui sait converser et que le compte sert. */
function modeleDeConversation(m: CloudModel): boolean {
  return (m.kind === undefined || m.kind === "chat") && m.available !== false;
}

/**
 * Choisir, pour ce tri seulement, un modèle chez un fournisseur (passerelle,
 * abonnement, clé d'API). Rien n'est retenu : la conversation garde son
 * modèle, et le prochain tri automatique suit les réglages du dossier.
 */
function ChoixModeleDistant({
  onLancer,
  onAnnuler,
}: {
  onLancer: (choix: ChoixDistant) => void;
  onAnnuler: () => void;
}) {
  const [fournisseurs, setFournisseurs] = useState<CloudProvider[] | null>(null);
  const [fournisseur, setFournisseur] = useState("");
  const [modeles, setModeles] = useState<CloudModel[] | null>(null);
  const [modele, setModele] = useState("");
  const [erreur, setErreur] = useState<string | null>(null);

  useEffect(() => {
    core
      .cloudProviders()
      .then((liste) => {
        const utiles = liste.filter((p) => p.model_count > 0);
        setFournisseurs(utiles);
        setFournisseur((f) => f || utiles[0]?.id || "");
      })
      .catch((e: unknown) => setErreur(messageDe(e)));
  }, []);

  useEffect(() => {
    if (!fournisseur) return;
    setModeles(null);
    setModele("");
    core
      .cloudProviderModels(fournisseur)
      .then((liste) => {
        const utiles = liste.filter(modeleDeConversation);
        setModeles(utiles);
        setModele(utiles[0]?.id ?? "");
      })
      .catch((e: unknown) => setErreur(messageDe(e)));
  }, [fournisseur]);

  if (fournisseurs && fournisseurs.length === 0) {
    return (
      <div className="locaryn-sort-pick">
        <p className="locaryn-sort-pick-note">
          Aucun modèle distant n'est disponible. Chargez un modèle local en lui envoyant un message,
          ou ajoutez un fournisseur depuis les extensions, puis relancez le tri.
        </p>
        <div className="locaryn-sort-pick-actions">
          <button type="button" className="locaryn-btn-ghost" onClick={onAnnuler}>
            Fermer
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="locaryn-sort-pick">
      <label>
        Fournisseur
        <select
          className="locaryn-input"
          value={fournisseur}
          disabled={!fournisseurs}
          onChange={(e) => setFournisseur(e.target.value)}
        >
          {(fournisseurs ?? []).map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
      </label>
      <label>
        Modèle
        <select
          className="locaryn-input"
          value={modele}
          disabled={!modeles || modeles.length === 0}
          onChange={(e) => setModele(e.target.value)}
        >
          {modeles === null && <option value="">Chargement…</option>}
          {modeles?.length === 0 && <option value="">Aucun modèle de conversation</option>}
          {(modeles ?? []).map((m) => (
            <option key={m.id} value={m.id}>
              {m.name || m.id}
            </option>
          ))}
        </select>
      </label>
      <div className="locaryn-sort-pick-actions">
        <button type="button" className="locaryn-btn-ghost" onClick={onAnnuler}>
          Annuler
        </button>
        <button
          type="button"
          className="locaryn-btn-primary"
          disabled={!fournisseur || !modele}
          onClick={() => onLancer({ provider: fournisseur, model: modele })}
        >
          Trier avec ce modèle
        </button>
      </div>
      <p className="locaryn-sort-pick-note">
        Pour ce tri seulement : la conversation garde son modèle. Toute la mémoire part chez ce
        fournisseur, et le tri consomme son quota.
      </p>
      {erreur && <div className="locaryn-vp-error">{erreur}</div>}
    </div>
  );
}

/**
 * Le tri forcé de la mémoire.
 *
 * Au repos, Locaryn trie tout seul quand le modèle ne sert plus à personne.
 * Ce bouton fait la même chose tout de suite et sans limite : toutes les
 * conversations sont relues, la mémoire est rangée, les phrases d'accueil sont
 * réécrites. Sans modèle local chargé ni fournisseur autorisé, le bouton reste
 * discret et propose un modèle distant pour ce tri seulement.
 */
export function MemorySortPanel({ onFinished }: { onFinished: () => void }) {
  const simple = useSimpleUi();
  const [status, setStatus] = useState<MemorySortStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pret, setPret] = useState<boolean | null>(null);
  const [choisir, setChoisir] = useState(false);
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
      setError(messageDe(e));
    }
  }, [onFinished]);

  // Un tri lancé avant l'ouverture de l'écran se retrouve en cours de route.
  useEffect(() => {
    void poll();
    core
      .memorySortReady()
      .then(setPret)
      .catch((e: unknown) => {
        console.warn("[mémoire] état du modèle de tri illisible :", e);
        setPret(false);
      });
    return () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, [poll]);

  async function lancer(choix?: ChoixDistant) {
    setError(null);
    try {
      await core.forceMemorySort(choix);
      setChoisir(false);
      wasRunning.current = true;
      void poll();
    } catch (e) {
      setError(messageDe(e));
    }
  }

  function cliquer() {
    if (pret === false) setChoisir((c) => !c);
    else void lancer();
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
          {pret === false && !running && (
            <p>Aucun modèle local n'est chargé : le tri se fera avec un modèle distant de votre choix.</p>
          )}
        </div>
        <button
          type="button"
          className={pret === false ? "locaryn-btn-ghost" : "locaryn-btn-primary"}
          disabled={running || pret === null}
          aria-expanded={pret === false ? choisir : undefined}
          onClick={cliquer}
        >
          <Icon name={running ? "refresh" : "sparkle"} size={15} />
          {running
            ? "Tri en cours…"
            : pret === false
              ? "Choisir un modèle…"
              : simple
                ? "Ranger"
                : "Trier"}
        </button>
      </div>

      {choisir && !running && (
        <ChoixModeleDistant onLancer={(c) => void lancer(c)} onAnnuler={() => setChoisir(false)} />
      )}

      {error && <div className="locaryn-vp-error">{error}</div>}

      {status && (running || status.finished) && <Avancement status={status} pct={pct} />}
    </section>
  );
}

function Avancement({ status, pct }: { status: MemorySortStatus; pct: number }) {
  return (
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
  );
}
