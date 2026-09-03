import { Icon } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { type ModelAbilities, core } from "../lib/core";
import { type RunView, clearRun, getRun, subscribeRun } from "../lib/runPanel";

/**
 * Ce qu'il faut dire du modèle chargé avant qu'on attende un artefact de lui.
 *
 * Un artefact vient d'un appel d'outil. Un modèle dont le gabarit de
 * conversation ne mentionne pas les outils n'en appellera jamais : il répondra
 * en prose, ce panneau restera vide, et rien n'aura dit pourquoi. Le bouton
 * reste donc actif — le panneau sert aussi à exécuter un bloc de code à la
 * main — mais il annonce la limite au lieu de la laisser découvrir.
 *
 * `inconnu` ne se dit pas comme `non` : beaucoup de fichiers GGUF ne déclarent
 * aucun gabarit, et le moteur en choisit un au chargement. Annoncer une
 * incapacité qu'on n'a pas vérifiée serait une erreur de plus, dans l'autre
 * sens.
 */
function AvertissementOutils({ abilities }: { abilities: ModelAbilities | null }) {
  if (!abilities || abilities.tools === "oui") return null;
  const inconnu = abilities.tools === "inconnu";
  return (
    <div className={`locaryn-run-notice${inconnu ? "" : " locaryn-run-notice-bad"}`} role="note">
      <Icon name={inconnu ? "info" : "warning"} size={15} />
      <div>
        <strong>
          {inconnu
            ? "Capacité aux outils non vérifiée"
            : "Le modèle chargé ne sait pas appeler d'outils"}
        </strong>
        <p>
          {inconnu ? (
            <>
              {abilities.model ?? "Ce modèle"} ne déclare pas de gabarit de conversation dans son
              fichier : impossible de dire d'ici s'il gère les outils. Chargez-le et la réponse
              deviendra certaine — le moteur dira alors quel gabarit il a retenu.
            </>
          ) : (
            <>
              Un artefact naît d'un appel d'outil, et le gabarit de {abilities.model ?? "ce modèle"}{" "}
              n'en prévoit aucun : il répondra en prose sans rien produire ici. Ce panneau reste
              utilisable pour exécuter un bloc de code à la main.
            </>
          )}
        </p>
      </div>
    </div>
  );
}

/**
 * The right-hand pane: what running a code block produced.
 *
 * Two shapes, because "run this" means two different things. A script produces
 * a stream of lines and an exit code, so it gets a terminal. A page produces
 * something to look at, so it gets rendered — sandboxed, since it comes from a
 * language model and may contain anything.
 */
export function RunPanel() {
  const [run, setRun] = useState<RunView | null>(getRun());
  const [abilities, setAbilities] = useState<ModelAbilities | null>(null);
  useEffect(() => subscribeRun(() => setRun(getRun())), []);
  // Relu à chaque ouverture du panneau : le modèle a pu changer depuis.
  useEffect(() => {
    let vivant = true;
    void core
      .modelAbilities()
      .then((a) => {
        if (vivant) setAbilities(a);
      })
      .catch(() => {
        // Capacités inconnues : on n'affiche aucun avertissement plutôt qu'un
        // avertissement dont on ne sait pas s'il est fondé.
      });
    return () => {
      vivant = false;
    };
  }, []);

  if (!run) {
    return (
      <aside className="locaryn-right">
        <AvertissementOutils abilities={abilities} />
        <div className="locaryn-run-empty">
          <div className="locaryn-run-empty-title">Rien à afficher</div>
          <div className="locaryn-run-empty-sub">
            Exécutez un bloc de code depuis une réponse : la sortie du terminal ou la page rendue
            s'affichera ici.
          </div>
        </div>
      </aside>
    );
  }

  return (
    <aside className="locaryn-right">
      <div className="locaryn-run-head">
        <span className="locaryn-run-title">
          {run.kind === "terminal" ? `Terminal · ${run.lang}` : run.title}
        </span>
        {run.kind === "terminal" && run.running && (
          <span className="locaryn-run-badge">en cours…</span>
        )}
        {run.kind === "terminal" && !run.running && (
          <span className={`locaryn-run-badge${run.exitCode ? " locaryn-run-badge-bad" : ""}`}>
            {run.exitCode ? `code ${run.exitCode}` : "terminé"}
          </span>
        )}
        <button
          type="button"
          className="locaryn-icon-btn"
          onClick={clearRun}
          aria-label="Fermer"
          title="Fermer"
        >
          <Icon name="close" size={16} />
        </button>
      </div>
      {run.kind === "terminal" ? <TerminalView run={run} /> : <WebView html={run.html} />}
    </aside>
  );
}

function TerminalView({ run }: { run: Extract<RunView, { kind: "terminal" }> }) {
  const ref = useRef<HTMLDivElement | null>(null);

  // Follow the tail, but only while the user is already at the bottom —
  // otherwise scrolling back to read an error fights the incoming output.
  // biome-ignore lint/correctness/useExhaustiveDependencies: les lignes ne sont pas lues ici, elles déclenchent : sans elles, la vue resterait immobile pendant que la sortie défile.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (el.scrollHeight - el.scrollTop - el.clientHeight < 60) {
      el.scrollTop = el.scrollHeight;
    }
  }, [run.lines]);

  return (
    <div className="locaryn-run-term" ref={ref}>
      <div className="locaryn-run-cwd">{run.cwd}</div>
      <div className="locaryn-run-cmd">$ {run.command}</div>
      {run.lines.map((l, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: sortie d'exécution en ajout seul ; deux lignes identiques sont courantes, l'index est le seul repère stable.
        <div className="locaryn-run-line" key={i}>
          {l}
        </div>
      ))}
      {run.running && <div className="locaryn-run-cursor" />}
      {!run.running && run.lines.length === 0 && (
        <div className="locaryn-run-line locaryn-run-dim">(aucune sortie)</div>
      )}
    </div>
  );
}

function WebView({ html }: { html: string }) {
  // `sandbox` without allow-same-origin: the document is model-generated, so it
  // must not reach this app's origin, storage or cookies. Scripts are allowed
  // so a demo page actually behaves like one, but they run walled off.
  return (
    <iframe className="locaryn-run-web" title="Aperçu" sandbox="allow-scripts" srcDoc={html} />
  );
}
