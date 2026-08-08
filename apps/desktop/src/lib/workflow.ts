// Plan-and-execute workflow: a substantial request is turned into a plan by the
// model, then executed step by step. When the model says the result must be
// verified (`needs_loop`), a failing verification restarts the plan and the
// notification center shows "essai 2", "essai 3"…
//
// Orchestration lives in the frontend on purpose: each step is a normal
// streamed chat turn, so the user sees the work happen in the conversation.

import { core, type StreamEvent } from "./core";
import { taskCenter } from "./taskCenter";

export interface WorkflowHooks {
  /** Forward stream events of the current step into the chat. */
  onEvent: (ev: StreamEvent) => void;
  /** Announce a step boundary in the conversation. */
  onStepStart?: (index: number, total: number, step: string, attempt: number) => void;
  /** Called when the whole run ends (success or exhausted retries). */
  onDone?: (ok: boolean, attempts: number) => void;
}

/** Max times the plan is replayed when its verification step fails. */
const MAX_ATTEMPTS = 3;

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
  return /(ne fonctionne (toujours )?pas|erreur persiste|toujours cass|\b[ée]chec\b|tests? (ont )?[ée]chou)/.test(a);
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
): Promise<boolean> {
  let plan;
  try {
    plan = await core.planTask(request);
  } catch {
    return false; // planning unavailable → plain answer
  }
  if (!plan.needs_plan || plan.steps.length === 0) return false;

  const taskId = taskCenter.addWorkflow(
    `Plan : ${request.slice(0, 40)}${request.length > 40 ? "…" : ""}`,
  );
  taskCenter.setPlan(taskId, plan.steps);

  let attempt = 1;
  let ok = false;

  while (attempt <= (plan.needs_loop ? MAX_ATTEMPTS : 1)) {
    if (attempt > 1) taskCenter.retryWorkflow(taskId);
    let failed = false;

    for (let i = 0; i < plan.steps.length; i++) {
      const step = plan.steps[i];
      hooks.onStepStart?.(i, plan.steps.length, step, attempt);

      // The last step of a looping plan is the verification: ask for a verdict
      // we can actually read back.
      const isCheck = plan.needs_loop && i === plan.steps.length - 1;
      const instruction = isCheck
        ? `${step}\n\nTermine ta réponse par une ligne "ok: true" si tout fonctionne, ou "ok: false" sinon.`
        : step;

      let answer = "";
      try {
        await core.sendMessage(
          sessionId,
          instruction,
          (ev) => {
            if (ev.type === "token") answer += ev.text;
            hooks.onEvent(ev);
          },
          undefined,
          null,
          null,
        );
      } catch {
        failed = true;
        break;
      }

      taskCenter.advanceStep(taskId);
      if (isCheck && verificationFailed(answer)) {
        failed = true;
        break;
      }
    }

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
