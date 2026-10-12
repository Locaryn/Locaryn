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

/** Source de l'avis qui explique l'arrêt d'un objectif qui n'avance plus. */
const TOUR_BLOQUE = "objectif_bloque";

/** Source de l'avis qui annonce le compte rendu d'un abandon. */
const TOUR_BILAN = "objectif_bilan";

/** Ce qu'on dit au modele quand il redit la meme chose : l'acte, pas l'intention. */
const INSISTANCE = [
  "[Objectif] Tu as redit la meme chose au lieu d'agir. N'annonce plus ce que tu vas faire : ",
  "appelle maintenant un outil (fichier, terminal, connecteur MCP...) et reponds avec ce qui ",
  "en resulte — le fichier ecrit, la commande lancee, le resultat obtenu. Si l'outil necessaire ",
  "manque ou ne repond pas, dis lequel et pourquoi : ne decris pas une intention.",
].join("");

/** Ce que l'arrêt dit à la personne. */
const ARRET_REPETITION =
  "l'objectif ne progresse plus : le modèle redit la même réponse sans agir. Arrêt — reformulez la demande, ou vérifiez que l'outil dont il a besoin est disponible.";

/** Un appel d'outil qui a échoué pendant l'objectif : le compte rendu s'appuie dessus. */
interface EchecOutil {
  tool: string;
  output: string;
}

/** La première ligne d'une sortie d'outil, bornée : un compte rendu se lit. */
function premiereLigne(texte: string): string {
  const ligne = texte.trim().split("\n")[0]?.trim() ?? "";
  return ligne.length > 200 ? `${ligne.slice(0, 200)}…` : ligne;
}

/**
 * Ce qu'on demande au modèle quand l'objectif est abandonné.
 *
 * Sans ce tour, l'abandon laissait la personne devant une carte « échec » et
 * douze tours dont elle ne pouvait rien tirer : impossible de savoir si un
 * connecteur MCP manquait, si un droit avait été refusé, ou si le modèle avait
 * simplement mal compris. Les échecs d'outils réels lui sont donnés, pour que
 * le compte rendu porte sur des faits et non sur une impression.
 */
function consigneBilan(
  goal: string,
  tour: number,
  reste: string,
  echecs: EchecOutil[],
  appels: number,
): string {
  // Trois cas très différents, qu'un même silence confondait : le modèle n'a
  // jamais rien appelé (il n'a fait qu'écrire), ses outils ont tous répondu, ou
  // certains ont échoué. C'est ce qui permet de dire si la panne est un
  // connecteur MCP, un droit refusé, ou une simple intention non exécutée.
  const liste = echecs.length
    ? echecs
        .slice(-8)
        .map((e) => `- ${e.tool} : ${premiereLigne(e.output) || "(sans message)"}`)
        .join("\n")
    : appels === 0
      ? "(aucun outil n'a été appelé de tout l'objectif : le modèle n'a fait qu'écrire du texte)"
      : "(les outils appelés ont répondu sans erreur)";
  return [
    `[Objectif] Arrêt de l'objectif « ${goal} » : il n'est pas atteint après ${tour} tour(s).`,
    "Fais maintenant le compte rendu pour la personne qui attendait, en quelques lignes :",
    "1. ce que tu as essayé et où ça s'est arrêté ;",
    "2. ce qui t'a bloqué, nommément — un outil ou un connecteur MCP qui n'a pas répondu, un droit refusé, un fichier ou une information qui manque, une commande qui échoue ;",
    "3. ce qu'il faut pour repartir, ou la décision que tu attends d'elle.",
    "",
    `Dernier verdict du juge : « ${reste} »`,
    `Appels d'outils pendant l'objectif : ${appels}`,
    "Appels d'outils qui ont échoué :",
    liste,
    "",
    "Ne relance pas de plan et ne promets rien : c'est un compte rendu, pas une nouvelle tentative.",
  ].join("\n");
}

/** Deux verdicts disent-ils la même chose ? Le juge n'est pas bavard : le tour
 *  qui n'avance pas revient avec la meme phrase. */
function memeVerdict(a: string, b: string): boolean {
  const cle = (s: string) => s.toLowerCase().replace(/\s+/g, " ").trim();
  const ca = cle(a);
  return ca !== "" && ca === cle(b);
}

/**
 * Deux réponses disent-elles la même chose ?
 *
 * Le juge ne suffit pas à repérer un objectif qui tourne : il varie sa formule
 * (« le jeu n'est pas construit », puis « la réponse doit décrire l'état
 * actuel ») alors que le modèle, lui, redit la même phrase d'un tour à
 * l'autre — parfois mot pour mot. On compare donc ce que le modèle a écrit, en
 * ignorant ponctuation, emoji et casse.
 */
function memeReponse(a: string, b: string): boolean {
  const cle = (s: string) =>
    s
      .toLowerCase()
      .replace(/\p{Extended_Pictographic}|\uFE0F|\u200D/gu, " ")
      .replace(/[^\p{L}\p{N}]+/gu, " ")
      .replace(/\s+/g, " ")
      .trim();
  const ca = cle(a);
  const cb = cle(b);
  if (ca === "" || cb === "") return false;
  // Redite mot pour mot : « D'abord, je vérifie l'état du studio : » deux fois
  // de suite ne dit rien d'autre qu'une boucle. Une salutation de deux mots
  // (« ok »), elle, ne prouve rien.
  if (ca === cb) return ca.length >= 12;
  // Reformulée à la marge : il faut de la matière pour comparer sans se tromper.
  if (ca.length < 40 || cb.length < 40) return false;
  const mots = (s: string) => new Set(s.split(" ").filter((m) => m.length > 3));
  const ma = mots(ca);
  const mb = mots(cb);
  if (ma.size === 0 || mb.size === 0) return false;
  let communs = 0;
  for (const m of ma) if (mb.has(m)) communs++;
  return communs / Math.max(ma.size, mb.size) >= 0.8;
}

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
  /** Le dernier verdict rendu, et depuis combien de tours il se répète. */
  let restePrecedent = "";
  let repetitions = 0;
  /** Le dernier verdict et son tour : le compte rendu d'un abandon les cite. */
  let dernierReste = "terminer et vérifier ce qui est demandé";
  let dernierTour = 0;
  /** Les outils appelés, et ceux qui ont échoué. */
  const outils = new Map<string, string>();
  const echecs: EchecOutil[] = [];
  /**
   * Le compte rendu d'un abandon : un dernier tour, non jugé, où le modèle dit
   * ce qui l'a bloqué. Un arrêt demandé par la personne ne le déclenche pas.
   */
  /** Les réponses déjà écrites pendant l'objectif : une redite est une boucle. */
  const reponsesVues: string[] = [];
  const bilan = async (tour: number, reste: string) => {
    hooks.onEvent({
      type: "log",
      level: "warn",
      msg: `Objectif non atteint après ${tour} tour(s) : le modèle fait le compte rendu de ce qui a bloqué.`,
      source: TOUR_BILAN,
    });
    try {
      await core.sendMessage(
        sessionId,
        consigneBilan(goal, tour, reste, echecs, outils.size),
        hooks.onEvent,
        undefined,
        null,
        null,
      );
    } catch (e) {
      // Un compte rendu impossible ne change rien : l'arrêt est déjà annoncé.
      console.warn("compte rendu de l'objectif impossible :", e);
    }
  };
  for (let tour = 1; tour <= toursMax; tour++) {
    dernierTour = tour;
    if (hooks.arret()) break;
    hooks.onTour(tour);
    let reponse = "";
    let coupe = false;
    /** Le tour a-t-il réellement appelé un outil ? Sinon il n'a fait qu'écrire. */
    const outilsDuTour = new Set<string>();
    await core.sendMessage(
      sessionId,
      message,
      (ev) => {
        if (ev.type === "token") reponse += ev.text;
        if (ev.type === "log" && ev.source === TOUR_INACHEVE) coupe = true;
        // Ce qui a échoué est noté pour le compte rendu : c'est souvent là
        // qu'est la panne (un connecteur MCP muet, un droit refusé).
        if (ev.type === "tool_call") {
          outils.set(ev.call_id, ev.tool);
          outilsDuTour.add(ev.tool);
        }
        if (ev.type === "tool_result" && !ev.ok) {
          echecs.push({ tool: outils.get(ev.call_id) ?? "outil", output: ev.output });
        }
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
    let atteint = false;
    let reste: string;
    if (coupe || !ecrit) {
      reste = "la tâche a été interrompue en cours de route : reprends là où tu t'es arrêté";
    } else {
      try {
        const verdict = await core.judgeGoal(goal, ecrit);
        atteint = verdict.atteint;
        reste = verdict.reste;
      } catch (e) {
        // Sans juge, on ne relance pas à l'aveugle : on s'arrête, on le dit, et
        // le modèle explique ce qu'il a fait de son côté.
        hooks.onVerdict(tour, false, `le juge est indisponible (${String(e)})`);
        await bilan(tour, `le juge est indisponible (${String(e)})`);
        taskCenter.fail(tache, "juge indisponible");
        return false;
      }
    }

    // Deux façons de tourner en rond, et il faut les deux : le juge peut redire
    // le même verdict, ou — plus souvent — varier sa formule pendant que le
    // modèle redonne la même réponse sans rien exécuter. Un tour qui appelle un
    // outil, lui, a agi : il compte comme un progrès. Au premier signe on
    // demande l'acte plutôt que l'intention ; au troisième on rend la main.
    const redite =
      ecrit.length > 0 &&
      outilsDuTour.size === 0 &&
      reponsesVues.some((precedente) => memeReponse(precedente, ecrit));
    const sansProgres = memeVerdict(restePrecedent, reste) || redite;
    repetitions = sansProgres ? repetitions + 1 : 0;
    if (ecrit) {
      reponsesVues.push(ecrit);
      if (reponsesVues.length > 5) reponsesVues.shift();
    }
    restePrecedent = reste;
    dernierReste = reste;
    const bloque = !atteint && repetitions >= 2;

    hooks.onVerdict(tour, atteint, bloque ? ARRET_REPETITION : reste);
    if (atteint) {
      taskCenter.done(tache, { detail: `atteint en ${tour} tour(s)` });
      return true;
    }
    if (bloque) {
      hooks.onEvent({ type: "log", level: "warn", msg: ARRET_REPETITION, source: TOUR_BLOQUE });
      await bilan(tour, reste);
      taskCenter.fail(tache, "pas de progrès");
      return false;
    }
    message = await core.goalRelance(reste || "terminer et vérifier ce qui est demandé");
    if (repetitions === 1) message += `\n\n${INSISTANCE}`;
  }
  // Plafond atteint : le modèle explique ce qui a bloqué, au lieu de laisser la
  // personne devant douze tours dont elle ne peut rien tirer. Un arrêt demandé
  // par la personne, lui, ne relance aucun tour.
  if (!hooks.arret()) await bilan(dernierTour, dernierReste);
  taskCenter.fail(tache, hooks.arret() ? "arrêté" : `pas atteint en ${toursMax} tours`);
  return false;
}
