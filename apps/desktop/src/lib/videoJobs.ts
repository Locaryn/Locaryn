import { toAudioUrl } from "./audioJobs";
import { core } from "./core";
import { taskCenter } from "./taskCenter";

export interface VideoJobResult {
  url: string;
  path: string;
}

export interface VideoGenParams {
  model: string;
  prompt: string;
  outputDir: string;
  /** Duration in seconds (for models that support it). */
  duration?: number;
  /** Input image path for image-to-video (Wan2.1 I2V, SVD). */
  inputImage?: string;
  /** Negative prompt (where supported). */
  negativePrompt?: string;
  /** Number of inference steps. */
  steps?: number;
  /** Guidance scale. */
  cfgScale?: number;
  /** Output width (where supported). */
  width?: number;
  /** Output height (where supported). */
  height?: number;
}

/** Start a video generation task in the background. Returns the task id. */
export function startVideoGeneration(p: VideoGenParams): string {
  const taskId = taskCenter.add({
    type: "generation",
    label: `Vidéo : ${p.prompt.slice(0, 42) || "sans prompt"}`,
    detail: "0s",
  });

  const t0 = Date.now();
  const timer = window.setInterval(() => {
    taskCenter.update(taskId, { detail: `${Math.round((Date.now() - t0) / 1000)}s` });
  }, 1000);

  core
    .generateVideo(
      p.model,
      p.prompt,
      p.outputDir,
      p.duration ?? null,
      p.inputImage ?? null,
      p.negativePrompt ?? null,
      p.steps ?? null,
      p.cfgScale ?? null,
      p.width ?? null,
      p.height ?? null,
      (pct, detail) => {
        taskCenter.update(taskId, { progress: pct, detail: detail ?? `${pct}%` });
      },
    )
    .then(async (res) => {
      window.clearInterval(timer);
      const secs = Math.round((Date.now() - t0) / 1000);
      const url = await toAudioUrl(res.path);
      taskCenter.done(taskId, {
        resultAudioUrl: url,
        resultPath: res.path,
        detail: `${secs}s`,
      });
    })
    .catch((e) => {
      window.clearInterval(timer);
      taskCenter.fail(taskId, String(e).replace(/^Error:\s*/, ""));
    });

  return taskId;
}
