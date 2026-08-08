import { core, type TtsSampling } from "./core";
import { loadMediaObjectUrl, toMediaUrl } from "./media";
import { taskCenter } from "./taskCenter";

export interface AudioJobResult {
  path: string;
  url: string;
  simulated: boolean;
}

export interface AudioGenParams {
  model: string;
  text: string;
  outputDir: string;
  voiceReference?: string;
  speaker?: string;
  speed?: number;
  /** Pitch shift: 0.5 = deeper, 2.0 = higher (default 1.0). */
  pitch?: number;
  /** Vocal energy / expressiveness: 0 = flat, 1 = full (default 0.7). */
  energy?: number;
  /** Articulation clarity: 0 = slurred, 1 = crisp (default 0.8). */
  clarity?: number;
  /** Synthesis language code (e.g. "fr", "en"). */
  language?: string;
  /** Voice design / style description (used when the engine supports it). */
  voiceDescription?: string;
  /** Detailed voice design prompt (used when the engine supports it). */
  designPrompt?: string;
  /** Sampling and cloning-style controls. Omitted means engine defaults. */
  sampling?: TtsSampling;
}

/** Convert a backend audio path to a playable URL.
 *
 *  Delegates to the shared media helper. The local copy this replaced used
 *  `/\\\\/g`, which matches *two* consecutive backslashes — a Windows path has
 *  one, so the separators were never converted and playback silently failed.
 *  On error it falls back to the plain asset URL, which at least lets the
 *  browser report a load failure instead of hanging on a blob that never came. */
export async function toAudioUrl(path: string): Promise<string> {
  try {
    return await loadMediaObjectUrl(path);
  } catch (err) {
    console.error("Audio blob load failed, falling back to the asset URL:", err);
    return toMediaUrl(path);
  }
}

/** Start a TTS generation in the background. Returns the task id. */
export function startAudioGeneration(p: AudioGenParams): string {
  const taskId = taskCenter.add({
    type: "audio",
    label: `TTS : ${p.text.slice(0, 42) || "sans texte"}`,
    detail: "0s",
  });

  const t0 = Date.now();
  const timer = window.setInterval(() => {
    taskCenter.update(taskId, { detail: `${Math.round((Date.now() - t0) / 1000)}s` });
  }, 1000);

  core.generateAudio(
      p.model,
      p.text,
      p.outputDir,
      p.voiceReference,
      p.speaker,
      p.speed,
      p.pitch,
      p.energy,
      p.clarity,
      p.language,
      p.voiceDescription,
      p.designPrompt,
      p.sampling,
      (pct, detail) => {
        taskCenter.update(taskId, { progress: pct, detail: detail ?? `${pct}%` });
      }
    )
    .then(async (res) => {
      window.clearInterval(timer);
      const secs = Math.round((Date.now() - t0) / 1000);
      const url = await toAudioUrl(res.path);
      taskCenter.done(taskId, {
        resultAudioUrl: url,
        resultPath: res.path,
        detail: `${secs}s${res.simulated ? " · simulé" : ""}`,
      });
    })
    .catch((e) => {
      window.clearInterval(timer);
      taskCenter.fail(taskId, String(e).replace(/^Error:\\s*/, ""));
    });

  return taskId;
}
