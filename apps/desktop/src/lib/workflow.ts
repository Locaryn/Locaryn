// Plan-and-execute workflow: a substantial request is turned into a plan by the
// model, then executed step by step. When the model says the result must be
// verified (`needs_loop`), a failing verification restarts the plan and the
// notification center shows "essai 2", "essai 3"…
//
// Orchestration lives in the frontend on purpose: each step is a normal
// streamed chat turn, so the user sees the work happen in the conversation.

import { type StreamEvent, type TaskPlan, core } from "./core";
import { taskCenter } from "./taskCenter";

export interface WorkflowHooks {
  /** Forward stream events of the current step into the chat. */
  onEvent: (ev: StreamEvent) => void;
  /** Announce a step boundary in the conversation. */
  onStepStart?: (index: number, total: number, step: string, attempt: number) => void;
  /** Called when the whole run ends (success or exhausted retries). */
  onDone?: (ok: boolean, attempts: number) => void;
  /** Le plan est connu : ses étapes et le nombre d'essais permis. */
  onPlan?: (steps: string[], maxAttempts: number) => void;
  /** Une étape s'est terminée ; `ok` faux quand la vérification a échoué. */
  onStepEnd?: (index: number, ok: boolean) => void;
  /** Vrai quand la personne a demandé l'arrêt : on ne lance plus d'étape. */
  arret?: () => boolean;
}

/** Reprises par defaut quand l'etape de verification echoue. */
const MAX_ATTEMPTS = 3;

/** Source de l'avis d'un tour fini sans reponse (limite d'actions atteinte). */
const TOUR_INACHEVE = "tour_inacheve";

/** Relances d'une etape coupee par la limite d'actions avant de la dire ratee. */
const SUITES_PAR_ETAPE = 3;

/** Ce que la personne a impose devant sa demande. */
export interface WorkflowOptions {
  /**
   * Imposer l'orchestration.
   *
   * Sans cela, un plan que le modele juge inutile fait retomber l'appelant sur
   * une reponse simple — ce qui est le bon defaut, mais pas ce que veut
   * quelqu'un qui a tape `/workflow`.
   */
  force?: boolean;
  /** Reprises voulues. Au-dela du defaut, et borne a cinq. */
  loops?: number | null;
}

/**
 * Did the verification step report a failure?
 *
 * The explicit verdict we asked for wins ("ok: true" / "ok: false"). Free-text
 * keywords are only a fallback, because words like "failed" appear in perfectly
 * successful answers ("the test failed at first, now it passes") and would
 * trigger pointless retries.
 */
function verificationFailed(answer: string): boolean {
  const a = answer.toLowerCase();
  const okTrue = /\bok\s*[:=]\s*"?true\b/.test(a);
  const okFalse = /\bok\s*[:=]\s*"?false\b/.test(a);
  if (okTrue) return false;
  if (okFalse) return true;
  // No verdict emitted — only clear, unambiguous failure statements count.
  return /(ne fonctionne (toujours )?pas|erreur persiste|toujours cass|\b[ée]chec\b|tests? (ont )?[ée]chou)/.test(
    a,
  );
}

/**
 * Run `request` as a plan when the model judges it warrants one.
 * Returns false when no plan was needed — the caller should then send the
 * request as a normal message.
 */
export async function runWorkflow(
  sessionId: string,
  request: string,
  hooks: WorkflowHooks,
  options: WorkflowOptions = {},
): Promise<boolean> {
  const force = options.force === true;
  let plan: TaskPlan;
  try {
    plan = await core.planTask(request, force);
  } catch {
    return false; // planning unavailable → plain answer
  }
  // Un plan sans etape n'est pas un plan : meme force, on rend la main plutot
  // que de lancer une orchestration vide. L'appelant dira alors pourquoi.
  if (plan.steps.length === 0) return false;
  if (!plan.needs_plan && !force) return false;

  // `/loop` demande une verification : sans elle, il n'y aurait rien a
  // reprendre, et la boucle ne servirait a rien.
  const loops = options.loops ?? null;
  if (loops !== null) plan = { ...plan, needs_loop: true };
  const maxAttempts = loops ?? MAX_ATTEMPTS;

  hooks.onPlan?.(plan.steps, plan.needs_loop ? maxAttempts : 1);
  const taskId = taskCenter.addWorkflow(
    `Plan : ${request.slice(0, 40)}${request.length > 40 ? "…" : ""}`,
  );
  taskCenter.setPlan(taskId, plan.steps);

  let attempt = 1;
  let ok = false;

  while (attempt <= (plan.needs_loop ? maxAttempts : 1)) {
    if (attempt > 1) taskCenter.retryWorkflow(taskId);
    let failed = false;

    for (let i = 0; i < plan.steps.length; i++) {
      if (hooks.arret?.()) {
        failed = true;
        break;
      }
      const step = plan.steps[i];
      hooks.onStepStart?.(i, plan.steps.length, step, attempt);

      // The last step of a looping plan is the verification: ask for a verdict
      // we can actually read back.
      const isCheck = plan.needs_loop && i === plan.steps.length - 1;
      const instruction = isCheck
        ? `${step}\n\nTermine ta réponse par une ligne "ok: true" si tout fonctionne, ou "ok: false" sinon.`
        : step;

      let answer = "";
      // Une etape coupee par la limite d'actions n'est pas faite : elle
      // passait pour terminee, et le plan avancait sur un travail a moitie
      // fait. Elle reprend la ou elle s'est arretee, quelques fois.
      let coupe = false;
      let suites = 0;
      let message = instruction;
      try {
        for (;;) {
          coupe = false;
          await core.sendMessage(
            sessionId,
            message,
            (ev) => {
              if (ev.type === "token") answer += ev.text;
              if (ev.type === "log" && ev.source === TOUR_INACHEVE) coupe = true;
              hooks.onEvent(ev);
            },
            undefined,
            null,
            null,
          );
          if (!coupe || suites >= SUITES_PAR_ETAPE || hooks.arret?.()) break;
          suites += 1;
          message = `Continue cette étape là où tu t'es arrêté, sans recommencer ce qui est fait : ${step}`;
        }
      } catch {
        hooks.onStepEnd?.(i, false);
        failed = true;
        break;
      }
      if (coupe) {
        hooks.onStepEnd?.(i, false);
        failed = true;
        break;
      }

      taskCenter.advanceStep(taskId);
      if (isCheck && verificationFailed(answer)) {
        hooks.onStepEnd?.(i, false);
        failed = true;
        break;
      }
      hooks.onStepEnd?.(i, true);
    }
    if (hooks.arret?.()) break;

    if (!failed) {
      ok = true;
      break;
    }
    attempt += 1;
  }

  if (ok) {
    taskCenter.done(taskId, { detail: attempt > 1 ? `réussi à l'essai ${attempt}` : "terminé" });
  } else {
    taskCenter.fail(taskId, `échec après ${attempt - 1} essai(s)`);
  }
  hooks.onDone?.(ok, attempt);
  return true;
}
