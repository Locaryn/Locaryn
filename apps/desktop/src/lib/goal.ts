// `/objectif` : le modèle travaille jusqu'à ce que les conditions écrites soient
// remplies, sans rendre la main entre deux tours. Après chaque réponse, un juge
// discret (agent-runtime::objectif) dit « atteint » ou ce qui reste ; ce reste
// relance le tour suivant. Le modèle peut toujours poser une question
// (ask_user) : elle s'affiche comme d'habitude, et le tour reprend après.
//
// L'orchestration vit dans l'interface, comme `runWorkflow` : chaque tour est
// une réponse ordinaire du fil, que la personne voit se faire.

import { type StreamEvent, core } from "./core";
import { taskCenter } from "./taskCenter";

/** Tours au plus avant de rendre la main : un objectif mal posé ne tourne pas sans fin. */
export const TOURS_MAX = 12;

/** Source de l'avis d'un tour fini sans réponse (`SOURCE_TOUR_INACHEVE`). */
const TOUR_INACHEVE = "tour_inacheve";

/** Ce que le modèle a écrit, sans sa réflexion : le juge lisait la fin d'un
 *  bloc `<think>` et la prenait pour un bilan. */
export function sansReflexion(texte: string): string {
  return texte
    .replace(/<think>[\s\S]*?<\/think>/g, "")
    .replace(/<think>[\s\S]*$/, "")
    .trim();
}

export interface GoalHooks {
  onEvent: (ev: StreamEvent) => void;
  /** Un tour commence (1, 2, 3…). */
  onTour: (tour: number) => void;
  /** Le juge a parlé sur ce tour. */
  onVerdict: (tour: number, atteint: boolean, reste: string) => void;
  /** Vrai quand la personne a demandé l'arrêt. */
  arret: () => boolean;
}

/** `true` si l'objectif est atteint, `false` sinon (arrêt, plafond, erreur). */
export async function runGoal(
  sessionId: string,
  goal: string,
  hooks: GoalHooks,
  toursMax = TOURS_MAX,
): Promise<boolean> {
  const tache = taskCenter.addWorkflow(
    `Objectif : ${goal.slice(0, 40)}${goal.length > 40 ? "…" : ""}`,
  );
  let message = await core.goalConsigne(goal);
  for (let tour = 1; tour <= toursMax; tour++) {
    if (hooks.arret()) break;
    hooks.onTour(tour);
    let reponse = "";
    let coupe = false;
    await core.sendMessage(
      sessionId,
      message,
      (ev) => {
        if (ev.type === "token") reponse += ev.text;
        if (ev.type === "log" && ev.source === TOUR_INACHEVE) coupe = true;
        hooks.onEvent(ev);
      },
      undefined,
      null,
      null,
    );
    if (hooks.arret()) break;
    // Un tour coupé par la limite d'actions, ou sans un mot de réponse, n'a
    // rien à juger : le travail était en cours. Le juge recevait une réponse
    // vide, et l'objectif pouvait s'arrêter là, en plein travail.
    const ecrit = sansReflexion(reponse);
    if (coupe || !ecrit) {
      const reste = "la tâche a été interrompue en cours de route : reprends là où tu t'es arrêté";
      hooks.onVerdict(tour, false, reste);
      message = await core.goalRelance(reste);
      continue;
    }
    let verdict: { atteint: boolean; reste: string };
    try {
      verdict = await core.judgeGoal(goal, ecrit);
    } catch (e) {
      // Sans juge, on ne relance pas à l'aveugle : on s'arrête et on le dit.
      hooks.onVerdict(tour, false, `le juge est indisponible (${String(e)})`);
      taskCenter.fail(tache, "juge indisponible");
      return false;
    }
    hooks.onVerdict(tour, verdict.atteint, verdict.reste);
    if (verdict.atteint) {
      taskCenter.done(tache, { detail: `atteint en ${tour} tour(s)` });
      return true;
    }
    message = await core.goalRelance(verdict.reste || "terminer et vérifier ce qui est demandé");
  }
  taskCenter.fail(tache, hooks.arret() ? "arrêté" : `pas atteint en ${toursMax} tours`);
  return false;
}
