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
//              and the run stops.
//
// Orchestration lives in the frontend: each step is a normal streamed turn.

import { type StreamEvent, type TaskPlan, core } from "./core";
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

/** Nombre max de retries par étape en mode goal. */
const MAX_STEP_RETRIES = 3;

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
 * Le verdict de vérification a-t-il signalé un échec ?
 *
 * Le verdict explicite `ok: true` / `ok: false` a la priorité. Les mots-clés
 * libres ne servent que de dernier recours, parce que des mots comme « failed »
 * apparaissent dans des réponses parfaitement réussies.
 */
function verificationFailed(answer: string): boolean {
  const a = answer.toLowerCase();
  const okTrue = /\bok\s*[:=]\s*"?true\b/.test(a);
  const okFalse = /\bok\s*[:=]\s*"?false\b/.test(a);
  if (okTrue) return false;
  if (okFalse) return true;
  return /(ne fonctionne (toujours )?pas|erreur persiste|toujours cass|\b[ée]chec\b|tests? (ont )?[ée]chou)/.test(
    a,
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

  hooks.onPlan?.(steps, goalMode ? MAX_STEP_RETRIES : 1);

  const taskId = taskCenter.addWorkflow(
    `Plan : ${request.slice(0, 40)}${request.length > 40 ? "…" : ""}`,
  );
  taskCenter.setPlan(taskId, steps);

  let globalOk = true;

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

    // En goalMode : on retente la même étape si elle échoue, jusqu'à
    // MAX_STEP_RETRIES. Sans goalMode, on exécute une seule fois.
    while (stepRetry <= (goalMode ? MAX_STEP_RETRIES : 0)) {
      if (hooks.arret?.()) {
        stepOk = false;
        break;
      }

      const step = steps[i];
      hooks.onStepStart?.(i, steps.length, step, stepRetry + 1);

      let answer = "";
      let stepError = false;
      try {
        await core.sendMessage(
          sessionId,
          step,
          (ev) => {
            if (ev.type === "token") answer += ev.text;
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

      if (stepError) {
        if (goalMode && stepRetry < MAX_STEP_RETRIES) {
          stepRetry++;
          continue; // retenter l'étape
        }
        globalOk = false;
        break;
      }

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

      // Validation en goalMode.
      if (goalMode) {
        let verdict = "";
        try {
          await core.sendMessage(
            sessionId,
            `${step}\n\nVérifie si cette étape est complètement accomplie. Termine ta réponse par une ligne "ok: true" si oui, ou "ok: false" si non.`,
            (ev) => {
              if (ev.type === "token") verdict += ev.text;
              hooks.onEvent(ev);
            },
            undefined,
            null,
            null,
          );
        } catch {
          // Vérification impossible → on admet la réussite pour ne pas bloquer.
          verdict = "ok: true";
        }

        if (verificationFailed(verdict)) {
          hooks.onStepEnd?.(i, false);
          if (stepRetry < MAX_STEP_RETRIES) {
            stepRetry++;
            continue; // retenter la même étape
          }
          // Max retries atteint : l'étape est marquée échouée, on arrête.
          globalOk = false;
          break;
        }
      }

      // Étape réussie.
      hooks.onStepEnd?.(i, true);
      taskCenter.advanceStep(taskId);
      stepOk = true;
      break;
    }

    if (!stepOk && !globalOk) break; // échec définitif sur cette étape
    i++;
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
