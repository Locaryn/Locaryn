// Background image generation: kicks off a generation detached from the modal
// so the popup can be closed while it runs. Progress shows in the notification
// center; the finished image is delivered to the active chat via a handler.

import { convertFileSrc } from "@tauri-apps/api/core";
import { core, type VramMode } from "./core";
import { taskCenter } from "./taskCenter";
import { recordImageGenerationDuration } from "./durationEstimator";

export type ImageJobResult = {
  prompt: string;
  /** Absolute disk path to the generated image. */
  path: string;
  /** Ready-to-display URL (asset://... in Tauri, data:... in demo). */
  url: string;
  simulated: boolean;
  /** Chat the generation was requested from — the result belongs to it. */
  sessionId: string | null;
};

let resultHandler: ((r: ImageJobResult) => void) | null = null;

/** Convert a backend image path to a displayable URL. Data URLs are returned
 *  as-is; disk paths are converted through the Tauri asset protocol after
 *  normalizing Windows backslashes. */
export function toImageUrl(path: string): string {
  if (path.startsWith("data:")) return path;
  // Normalize Windows backslashes and encode spaces / special characters so
  // the Tauri asset protocol can resolve the file correctly.
  return convertFileSrc(encodeURI(path.replace(/\\/g, "/")));
}

/** ChatPanel registers this so finished images land in the current chat. */
export function setImageResultHandler(fn: ((r: ImageJobResult) => void) | null) {
  resultHandler = fn;
}

export interface ImageJobParams {
  model: string;
  prompt: string;
  outputDir: string;
  inputImage?: string;
  negativePrompt?: string;
  steps?: number;
  cfgScale?: number;
  width?: number;
  height?: number;
  vramMode?: VramMode;
  uncensored?: boolean;
  /** User accepted the NSFW / unfiltered checkpoint responsibility gate. */
  consent?: boolean;
  /** Render this many variants in one run (1-8). The model load and prompt
   *  encoding are paid once, so extra variants are much cheaper than extra
   *  runs — measured 40 s each instead of 58 s on Z-Image Turbo. */
  variants?: number;
  /** Chat that requested this generation, so the image lands in the right one. */
  sessionId?: string | null;
}

/** The most recent image job's full input context, so reopening the popup from
 *  the notification center restores exactly what the user was doing (prompt,
 *  source image, model, mode) and shows the live progress. */
export interface ActiveImageJob {
  taskId: string;
  params: ImageJobParams;
  mode: "txt2img" | "img2img";
  running: boolean;
}

let activeJob: ActiveImageJob | null = null;
const activeListeners = new Set<() => void>();
function emitActive() {
  activeListeners.forEach((l) => l());
}
export function getActiveImageJob(): ActiveImageJob | null {
  return activeJob;
}
export function subscribeActiveImageJob(l: () => void): () => void {
  activeListeners.add(l);
  return () => activeListeners.delete(l);
}

/** Start a generation in the background. Returns immediately with the task id. */
export function startImageGeneration(p: ImageJobParams): string {
  const isEdit = Boolean(p.inputImage);
  const taskId = taskCenter.add({
    type: isEdit ? "edit" : "generation",
    label: `${isEdit ? "Édition" : "Image"} : ${p.prompt.slice(0, 42) || "sans prompt"}`,
    detail: "0s",
  });
  activeJob = { taskId, params: p, mode: isEdit ? "img2img" : "txt2img", running: true };
  emitActive();

  const t0 = Date.now();
  // Fallback ticker (elapsed seconds) until real step progress arrives.
  let gotStep = false;
  const timer = window.setInterval(() => {
    if (!gotStep) taskCenter.update(taskId, { detail: `${Math.round((Date.now() - t0) / 1000)}s` });
  }, 1000);

  core
    .generateImage(
      p.model, p.prompt, p.outputDir, p.inputImage, p.negativePrompt,
      p.steps, p.cfgScale, p.width, p.height, p.vramMode, p.uncensored, p.consent,
      p.variants,
      (pct, detail) => {
        gotStep = true;
        taskCenter.update(taskId, { progress: pct, detail: detail ?? `${pct}%` });
      },
    )
    .then((res) => {
      window.clearInterval(timer);
      const durationMs = Date.now() - t0;
      const secs = Math.round(durationMs / 1000);
      // The backend now returns a disk path. Convert it to a displayable URL
      // (asset://... in Tauri, data:... in demo) so we never push base64
      // payloads through IPC or SQLite.
      const normalizedPath = res.path.replace(/\\/g, "/");
      const displayUrl = toImageUrl(res.path);
      // Feed the estimator so the UI can predict future durations.
      if (!res.simulated) {
        recordImageGenerationDuration(
          {
            model: p.model,
            vramMode: p.vramMode ?? "auto",
            mode: isEdit ? "img2img" : "txt2img",
            steps: p.steps ?? 20,
            width: p.width ?? 512,
            height: p.height ?? 512,
          },
          durationMs,
        );
      }

      // Several variants may come back from a single run; keep them all so the
      // user can pick rather than only seeing the first.
      const allUrls = (res.variants?.length ? res.variants : [res.path]).map(toImageUrl);
      taskCenter.done(taskId, {
        resultImageUrl: displayUrl,
        resultPath: normalizedPath,
        detail:
          `${secs}s` +
          (allUrls.length > 1 ? ` · ${allUrls.length} variantes` : "") +
          (res.simulated ? " · simulé" : ""),
      });
      if (activeJob?.taskId === taskId) { activeJob = { ...activeJob, running: false }; emitActive(); }
      // Persist into the requesting chat so the image survives switching chats.
      if (p.sessionId) {
        const heading =
          allUrls.length > 1
            ? `🎨 ${allUrls.length} variantes — « ${p.prompt} »`
            : `🎨 ${res.simulated ? "(simulation) " : ""}Image générée — « ${p.prompt} »`;
        const md = `${heading}\n\n${allUrls.map((u) => `![](${u})`).join("\n")}`;
        core.appendAssistantMessage(p.sessionId, md).catch((err) => {
          // Do not swallow persistence errors; they make it look like the
          // generation history was lost. Surface it in the notification center
          // and the browser console so the user knows the image itself is safe
          // on disk, but the chat history line was not saved.
          // eslint-disable-next-line no-console
          console.error("[imageJobs] appendAssistantMessage failed:", err);
          taskCenter.update(taskId, {
            detail: `${secs}s · historique non enregistré`,
          });
        });
      }
      resultHandler?.({
        prompt: p.prompt,
        path: normalizedPath,
        url: displayUrl,
        simulated: res.simulated,
        sessionId: p.sessionId ?? null,
      });
    })
    .catch((e) => {
      window.clearInterval(timer);
      taskCenter.fail(taskId, String(e).replace(/^Error:\s*/, ""));
      if (activeJob?.taskId === taskId) { activeJob = { ...activeJob, running: false }; emitActive(); }
    });

  return taskId;
}
