import { toAudioUrl } from "./audioJobs";
import { core } from "./core";
import { taskCenter } from "./taskCenter";

export interface MusicJobResult {
  url: string;
  path: string;
}

export interface MusicGenParams {
  model: string;
  prompt: string;
  outputDir: string;
  duration?: number;
  /** Reference audio path for melody conditioning (MusicGen Melody). */
  melodyReference?: string;
  /** Negative / undesired content prompt (where supported). */
  negativePrompt?: string;
  /** Number of generation steps (higher = better quality but slower). */
  steps?: number;
  /** Guidance scale (cfg). Higher = closer to prompt but less creative. */
  cfgScale?: number;
}

/** Start a music generation task in the background. Returns the task id. */
export function startMusicGeneration(p: MusicGenParams): string {
  const taskId = taskCenter.add({
    type: "audio",
    label: `Musique : ${p.prompt.slice(0, 42) || "sans prompt"}`,
    detail: "0s",
  });

  const t0 = Date.now();
  const timer = window.setInterval(() => {
    taskCenter.update(taskId, { detail: `${Math.round((Date.now() - t0) / 1000)}s` });
  }, 1000);

  core
    .generateMusic(
      p.model,
      p.prompt,
      p.outputDir,
      p.duration ?? null,
      p.melodyReference ?? null,
      p.negativePrompt ?? null,
      p.steps ?? null,
      p.cfgScale ?? null,
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
