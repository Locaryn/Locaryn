import { core } from "./core";
import { taskCenter } from "./taskCenter";
import { toAudioUrl } from "./audioJobs";

export interface Model3DJobResult {
  url: string;
  path: string;
}

export interface Model3DGenParams {
  model: string;
  prompt: string;
  outputDir: string;
  /** Input image path for image-to-3d (TripoSR). */
  inputImage?: string;
  /** Negative prompt (where supported). */
  negativePrompt?: string;
  /** Number of inference steps. */
  steps?: number;
  /** Guidance scale. */
  cfgScale?: number;
  /** Output format: "obj" | "glb" | "ply". */
  format?: string;
}

/** Start a 3D model generation task in the background. Returns the task id. */
export function startModel3DGeneration(p: Model3DGenParams): string {
  const taskId = taskCenter.add({
    type: "generation",
    label: `3D : ${p.prompt.slice(0, 42) || "sans prompt"}`,
    detail: "0s",
  });

  const t0 = Date.now();
  const timer = window.setInterval(() => {
    taskCenter.update(taskId, { detail: `${Math.round((Date.now() - t0) / 1000)}s` });
  }, 1000);

  core
    .generate3D(
      p.model,
      p.prompt,
      p.outputDir,
      p.inputImage ?? null,
      p.negativePrompt ?? null,
      p.steps ?? null,
      p.cfgScale ?? null,
      p.format ?? null,
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
