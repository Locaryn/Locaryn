// Plan-and-execute workflow: a substantial request is turned into a plan by the
// model (all steps at once, upfront), then executed step by step.
//
// Two independent flags can be combined:
//  - `/plan`  → decompose upfront, show all steps, execute sequentially.
//              If during an execution the model response signals a step split
//              (via a fenced JSON block `{"split":["sub1","sub2",...]}`) the
//              sub-steps are inserted in-place so the list grows automatically.
//  - `/goal`  → after each step, ask the model to verify the result.
//              On failure, the *failing step* is retried (not the whole plan).
//              After MAX_STEP_RETRIES failed retries the step is marked failed
//              and the plan goes on. Once every step has run, a final step
//              checks the *whole request* (every condition), and comes back
//              with what is still missing until it is reached
//              (MAX_VERIFICATIONS_FINALES times at most).
//
// Orchestration lives in the frontend: each step is a normal streamed turn.

import { type StreamEvent, type TaskPlan, core } from "./core";
import { sansReflexion } from "./goal";
import { taskCenter } from "./taskCenter";

export interface WorkflowHooks {
  /** Forward stream events of the current step into the chat. */
  onEvent: (ev: StreamEvent) => void;
  /** Announce a step boundary in the conversation. */
  onStepStart?: (index: number, total: number, step: string, attempt: number) => void;
  /** Called when the whole run ends (success or exhausted retries). */
  onDone?: (ok: boolean, attempts: number) => void;
  /** Le plan est connu : ses étapes et le nombre max de retries par étape. */
  onPlan?: (steps: string[], maxStepRetries: number) => void;
  /** Une étape s'est terminée ; `ok` faux quand la vérification a échoué. */
  onStepEnd?: (index: number, ok: boolean) => void;
  /**
   * Le plan a été modifié (étape scindée) : les nouvelles étapes complètes
   * remplacent la liste initiale. L'UI peut ainsi redessiner la carte.
   */
  onPlanUpdated?: (steps: string[]) => void;
  /** Vrai quand la personne a demandé l'arrêt : on ne lance plus d'étape. */
  arret?: () => boolean;
}

/**
 * Reprises d'une étape coupée par la limite d'actions d'un message (20) :
 * elles ne comptent pas comme des essais. Une étape de plan qui écrit un
 * site entier dépasse vite vingt actions ; s'arrêter là laissait l'étape
 * « non aboutie » et tout le plan avec elle.
 */
const MAX_SUITES = 8;

/** La source de l'avis d'un tour coupé (`SOURCE_TOUR_INACHEVE` du runtime). */
const TOUR_INACHEVE = "tour_inacheve";

/** Nombre max de retries par étape en mode goal. */
const MAX_STEP_RETRIES = 3;

/**
 * Vérifications finales au plus, chacune avec ses propres reprises.
 *
 * Chaque étape validée une à une ne dit pas que la demande l'est : la
 * vérification finale juge contre la demande entière — toutes ses
 * conditions — et revient sur ce qui manque.
 */
const MAX_VERIFICATIONS_FINALES = 3;

/**
 * Le marqueur des consignes que l'application envoie elle-même au modèle (une
 * étape de plan, sa reprise). Elles sont enregistrées dans la conversation —
 * le modèle doit les relire — mais ne s'affichent pas : seul ce que la
 * personne a tapé apparaît comme venant d'elle.
 */
const MARQUE_INTERNE = "<!--locaryn-interne-->";

/** Une consigne interne, marquée ou d'avant le marqueur (« [Étape 2/5 du plan] »). */
export function estConsigneInterne(contenu: string): boolean {
  const t = contenu.trimStart();
  return t.startsWith(MARQUE_INTERNE) || /^(?:@[\w.-]+\s*)*\[Étape \d+\/\d+ du plan\]/u.test(t);
}

/** Après une panne du moteur, on attend avant de relancer : 5 s, 15 s, 30 s. */
const ATTENTES_APRES_PANNE_MS = [5000, 15000, 30000];

/** Pannes du moteur de suite sur une étape avant d'arrêter le plan. */
const MAX_PANNES = 3;

function attendre(ms: number, arret?: () => boolean): Promise<void> {
  return new Promise((fin) => {
    const debut = Date.now();
    const tic = setInterval(() => {
      if (Date.now() - debut >= ms || arret?.()) {
        clearInterval(tic);
        fin();
      }
    }, 250);
  });
}

/** L'étape ajoutée en fin de plan, en mode goal. */
const ETAPE_VERIFICATION_FINALE =
  "Vérification finale : contrôle concrètement chaque condition de la demande globale (lis les fichiers, lance ou ouvre ce qui doit l'être), corrige tout ce qui manque ou ne marche pas, puis liste chaque condition avec son état vérifié.";

/** Ce que la personne a imposé devant sa demande. */
export interface WorkflowOptions {
  /**
   * Imposer l'orchestration.
   *
   * Sans cela, un plan que le modèle juge inutile fait retomber l'appelant sur
   * une réponse simple — ce qui est le bon défaut, mais pas ce que veut
   * quelqu'un qui a tapé `/plan`.
   */
  force?: boolean;
  /**
   * Activer la validation par étape (/goal).
   *
   * Après chaque étape, le modèle est interrogé (verdict ok/fail). En cas
   * d'échec, **la même étape** est relancée (pas le plan entier) jusqu'à
   * MAX_STEP_RETRIES fois.
   */
  goalMode?: boolean;
  /** @deprecated - Ancienne option de loop globale. Ignorée. */
  loops?: number | null;
}

/**
 * Tente d'extraire des sous-étapes d'une réponse de modèle.
 *
 * Si la réponse contient un bloc JSON de la forme :
 *   ```json
 *   {"split":["sous-étape 1","sous-étape 2"]}
 *   ```
 * ces sous-étapes remplacent l'étape en cours dans le plan.
 * Renvoie `null` si aucune scission n'est demandée.
 */
function extractSplit(answer: string): string[] | null {
  const match = answer.match(/```(?:json)?\s*(\{"split"\s*:\s*\[[\s\S]*?\]})\s*```/i);
  if (!match) return null;
  try {
    const parsed = JSON.parse(match[1]) as unknown;
    if (
      parsed &&
      typeof parsed === "object" &&
      "split" in parsed &&
      Array.isArray((parsed as { split: unknown }).split)
    ) {
      const subs = (parsed as { split: unknown[] }).split.filter(
        (s): s is string => typeof s === "string" && s.trim().length > 0,
      );
      if (subs.length >= 2) return subs;
    }
  } catch {
    // JSON invalide → pas de scission
  }
  return null;
}

/**
 * L'état réel du dossier de la conversation, en quelques lignes : chaque
 * fichier avec sa taille, trois niveaux au plus.
 *
 * Sans lui, une étape reprise ne savait plus ce que les essais précédents
 * avaient écrit : le modèle recommençait le site dans un autre dossier, et le
 * juge ne jugeait que ce que le modèle disait avoir fait.
 */
async function etatDuDossier(sessionId: string, maxEntrees = 60): Promise<string> {
  const lignes: string[] = [];
  const parcourir = async (dossier: string, profondeur: number) => {
    if (profondeur > 3 || lignes.length >= maxEntrees) return;
    let entrees: Awaited<ReturnType<typeof core.workspaceList>>;
    try {
      entrees = await core.workspaceList(sessionId, dossier);
    } catch {
      return;
    }
    const tries = [...entrees].sort((a, b) =>
      a.dossier === b.dossier ? a.nom.localeCompare(b.nom) : a.dossier ? 1 : -1,
    );
    for (const e of tries) {
      if (lignes.length >= maxEntrees) {
        lignes.push("…");
        return;
      }
      if (e.dossier) {
        lignes.push(`${e.chemin}/`);
        await parcourir(e.chemin, profondeur + 1);
      } else {
        const ko = e.taille >= 1024 ? `${(e.taille / 1024).toFixed(1)} Ko` : `${e.taille} o`;
        lignes.push(`${e.chemin} (${ko})`);
      }
    }
  };
  await parcourir(".", 1);
  return lignes.length ? lignes.join("\n") : "(dossier vide)";
}

/**
 * Le modèle a-t-il seulement annoncé ce qu'il allait faire ?
 *
 * Un petit modèle local répond volontiers « Je vais d'abord vérifier le
 * chemin du fichier… » et rend la main sans appeler d'outil. Ce n'est ni un
 * échec ni une étape faite : on lui demande de le faire, sans compter d'essai.
 */
function simpleAnnonce(reponse: string, outilsAppeles: number): boolean {
  if (outilsAppeles > 0) return false;
  const texte = sansReflexion(reponse).trim();
  if (texte.length === 0 || texte.length > 700) return texte.length === 0;
  return (
    /(?:^|[^\p{L}])(je vais|je dois|je m'apprête|commençons|je commence|laisse-moi|d'abord|let me|i will|i'll|first,)/iu.test(
      texte,
    ) || /[:：]\s*$/.test(texte)
  );
}

/**
 * Exécute `request` comme plan quand le modèle estime qu'il en vaut un.
 * Renvoie `false` quand aucun plan n'est nécessaire — l'appelant doit alors
 * traiter la demande comme un message ordinaire.
 *
 * Comportement :
 *  1. Toutes les étapes sont générées en une seule passe (upfront).
 *  2. Elles sont affichées à l'utilisateur via `hooks.onPlan`.
 *  3. Chaque étape est exécutée séquentiellement.
 *  4. Si la réponse contient un bloc `{"split":[…]}`, les sous-étapes
 *     remplacent l'étape courante et le plan est mis à jour (onPlanUpdated).
 *  5. En `goalMode`, après chaque étape, le modèle émet un verdict :
 *       - ok: true  → étape réussie, passage à la suivante.
 *       - ok: false → l'étape est relancée (pas le plan entier) jusqu'à
 *                     MAX_STEP_RETRIES fois.
 */
export async function runWorkflow(
  sessionId: string,
  request: string,
  hooks: WorkflowHooks,
  options: WorkflowOptions = {},
): Promise<boolean> {
  const force = options.force === true;
  const goalMode = options.goalMode === true;

  let plan: TaskPlan;
  try {
    plan = await core.planTask(request, force);
  } catch {
    return false; // planification indisponible → réponse simple
  }

  // Un plan sans étape n'est pas un plan : même forcé, on rend la main plutôt
  // que de lancer une orchestration vide.
  if (plan.steps.length === 0) return false;
  if (!plan.needs_plan && !force) return false;

  // Copie mutable des étapes — le plan peut s'allonger par scission.
  const steps = [...plan.steps];

  // Le nombre d'essais affiché : le premier, plus les reprises. Passer les
  // seules reprises faisait lire « essai 4/3 ».
  hooks.onPlan?.(steps, goalMode ? MAX_STEP_RETRIES + 1 : 1);

  const taskId = taskCenter.addWorkflow(
    `Plan : ${request.slice(0, 40)}${request.length > 40 ? "…" : ""}`,
  );
  taskCenter.setPlan(taskId, steps);

  let globalOk = true;
  let etapesEchouees = 0;
  let objectifAtteint = false;
  // Les indices des étapes de vérification finale : leur juge regarde la
  // demande entière, pas l'étape.
  const finales = new Set<number>();
  let verificationsFinales = 0;
  const mentions = (request.match(/@[\w.-]+/g) ?? []).join(" ");

  // Boucle sur les étapes : `i` avance, mais peut être décrémenté si
  // l'insertion de sous-étapes le demande. C'est une boucle while car la
  // longueur de `steps` peut croître.
  let i = 0;
  while (i < steps.length) {
    if (hooks.arret?.()) {
      globalOk = false;
      break;
    }

    let stepRetry = 0;
    let stepOk = false;
    let lastReste = "";
    // Relances après une simple annonce, sans compter d'essai (bornées).
    let relances = 0;
    let suites = 0;
    let suiteEnCours = false;
    let relanceEnCours = false;
    let pannes = 0;

    // En goalMode : on retente la même étape si elle échoue, jusqu'à
    // MAX_STEP_RETRIES. Sans goalMode, on exécute une seule fois.
    while (stepRetry <= (goalMode ? MAX_STEP_RETRIES : 0)) {
      if (hooks.arret?.()) {
        stepOk = false;
        break;
      }

      const step = steps[i];
      hooks.onStepStart?.(i, steps.length, step, stepRetry + 1);
      const dossierAvant = await etatDuDossier(sessionId);

      const stepPrompt = [
        MARQUE_INTERNE,
        mentions ? `${mentions}` : "",
        `[Étape ${i + 1}/${steps.length} du plan] : ${step}`,
        `Demande globale : ${request}`,
        `État actuel du dossier du projet (ce qui existe déjà — pars de là, ne recommence pas ailleurs) :\n${dossierAvant}`,
        (stepRetry > 0 || relanceEnCours || suiteEnCours) && lastReste
          ? `[Correction / Ce qui reste à faire pour valider cette étape] : ${lastReste}`
          : "",
        `Consigne impérative : Réalise cette étape de manière concrète et autonome en utilisant directement tes outils. Ne pose pas de question d'attente à l'utilisateur, ne t'arrête pas en chemin et ne demande pas de confirmation : accomplis le travail requis pour cette étape.`,
      ]
        .filter(Boolean)
        .join("\n\n");

      let answer = "";
      let outilsAppeles = 0;
      let coupe = false;
      let stepError = false;
      let panne = false;
      try {
        await core.sendMessage(
          sessionId,
          stepPrompt,
          (ev) => {
            if (ev.type === "token") answer += ev.text;
            if (ev.type === "tool_call") outilsAppeles++;
            if (ev.type === "log" && ev.source === TOUR_INACHEVE) coupe = true;
            if (ev.type === "log" && ev.level === "error") panne = true;
            hooks.onEvent(ev);
          },
          undefined,
          null,
          null,
        );
      } catch {
        hooks.onStepEnd?.(i, false);
        stepError = true;
      }

      // Le moteur est tombé (ou a été éjecté) sans rien produire : ce n'est
      // pas une réponse à juger. Sans cela, une réponse vide passait pour une
      // « simple annonce », puis un échec, et toutes les étapes partaient à la
      // suite en quelques secondes, sans rien faire.
      if (!stepError && panne && outilsAppeles === 0 && !sansReflexion(answer).trim()) {
        stepError = true;
      }

      if (stepError) {
        pannes++;
        if (pannes >= MAX_PANNES || hooks.arret?.()) {
          globalOk = false;
          break;
        }
        await attendre(
          ATTENTES_APRES_PANNE_MS[Math.min(pannes - 1, ATTENTES_APRES_PANNE_MS.length - 1)],
          hooks.arret,
        );
        if (hooks.arret?.()) {
          globalOk = false;
          break;
        }
        // Une panne n'est pas un essai raté : on reprend la même étape, borné
        // par MAX_PANNES, avec ou sans /goal.
        lastReste = "Une erreur technique a interrompu l'étape : reprends-la là où elle en était.";
        suiteEnCours = true;
        continue;
      }
      pannes = 0;

      // Coupée par la limite d'actions : on reprend la même étape là où elle
      // s'est arrêtée, sans juger un travail inachevé ni compter d'essai.
      if (coupe && suites < MAX_SUITES && !hooks.arret?.()) {
        suites++;
        lastReste =
          "La limite d'actions du message précédent a coupé ton travail. Continue cette étape exactement là où tu t'es arrêté (l'état du dossier ci-dessus montre ce qui existe déjà), sans recommencer ce qui est fait.";
        suiteEnCours = true;
        continue;
      }
      suiteEnCours = false;

      if (simpleAnnonce(answer, outilsAppeles) && relances < 2 && !hooks.arret?.()) {
        relances++;
        lastReste =
          "Tu as annoncé ce que tu allais faire sans le faire. Fais-le maintenant, directement avec tes outils, puis rends compte du résultat.";
        relanceEnCours = true;
        continue;
      }
      relanceEnCours = false;

      // Scission dynamique : le modèle peut signaler que l'étape doit être
      // découpée en sous-étapes. Si c'est le cas, on remplace l'étape
      // courante dans le plan et on réexécute à partir de la première
      // sous-étape (sans compter le split comme une tentative).
      const split = extractSplit(answer);
      if (split && split.length >= 2) {
        steps.splice(i, 1, ...split);
        hooks.onPlanUpdated?.(steps);
        // On ne comptabilise pas la tentative : on relance l'étape i (qui est
        // maintenant la première sous-étape) sans toucher stepRetry.
        continue;
      }

      // Validation discrète en goalMode via le juge d'objectif.
      if (goalMode) {
        let reached = true;
        let reste = "";
        try {
          // Le juge voit aussi les fichiers réellement présents : une réponse
          // qui affirme « c'est fait » ne suffit pas.
          const dossierApres = await etatDuDossier(sessionId);
          const verdict = await core.judgeGoal(
            finales.has(i) ? request : step,
            `${answer}\n\n[Fichiers réellement présents dans le dossier du projet]\n${dossierApres}`,
          );
          reached = verdict.atteint;
          reste = verdict.reste;
        } catch {
          // Si le juge est momentanément indisponible, on valide pour ne pas bloquer.
          reached = true;
        }

        if (!reached) {
          hooks.onStepEnd?.(i, false);
          if (stepRetry < MAX_STEP_RETRIES) {
            stepRetry++;
            lastReste =
              reste ||
              "Poursuis et finalise les actions concrètes nécessaires pour valider cette étape.";
            continue; // retenter la même étape avec les indications du juge
          }
          // Reprises épuisées : l'étape est marquée échouée, mais le plan
          // continue — la vérification finale revient sur ce qui manque.
          // S'arrêter là laissait le reste du travail jamais fait.
          etapesEchouees++;
          break;
        }
      }

      // Étape réussie.
      hooks.onStepEnd?.(i, true);
      taskCenter.advanceStep(taskId);
      stepOk = true;
      if (finales.has(i)) {
        // La demande entière est jugée remplie : ce qui avait échoué en
        // route a été rattrapé.
        objectifAtteint = true;
      }
      break;
    }

    if (!stepOk && !globalOk) break; // erreur technique définitive
    i++;

    // Fin du plan en mode goal : vérifier la demande entière, et y revenir
    // tant qu'elle n'est pas remplie (borné).
    if (
      goalMode &&
      i >= steps.length &&
      !objectifAtteint &&
      verificationsFinales < MAX_VERIFICATIONS_FINALES &&
      !hooks.arret?.()
    ) {
      verificationsFinales++;
      finales.add(steps.length);
      steps.push(ETAPE_VERIFICATION_FINALE);
      hooks.onPlanUpdated?.(steps);
      taskCenter.setPlan(taskId, steps);
    }
  }
  if (goalMode) {
    // Le résultat est celui de la demande entière.
    globalOk = globalOk && objectifAtteint;
  } else if (etapesEchouees > 0) {
    globalOk = false;
  }

  if (globalOk && !hooks.arret?.()) {
    taskCenter.done(taskId, { detail: "terminé" });
  } else if (hooks.arret?.()) {
    taskCenter.done(taskId, { detail: "arrêté" });
  } else {
    taskCenter.fail(taskId, "échec d'une étape");
  }

  hooks.onDone?.(globalOk && !hooks.arret?.(), 1);
  return true;
}
