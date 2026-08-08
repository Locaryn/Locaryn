import { useEffect, useMemo, useRef, useState } from "react";
import { core } from "../lib/core";
import { startMusicGeneration, type MusicJobResult } from "../lib/musicJobs";
import { taskCenter } from "../lib/taskCenter";
import { dedupeModelsByDirectory } from "../lib/modelList";
import { isMusicGenModel } from "../lib/modelRegistry";
import { toMediaUrl } from "../lib/media";

// ── Types ──────────────────────────────────────────────────────────────────

type Props = {
  installedModels: string[];
  onClose: () => void;
  inline?: boolean;
};

const DURATION_PRESETS = [
  { label: "15s", value: 15 },
  { label: "30s", value: 30 },
  { label: "60s", value: 60 },
  { label: "90s", value: 90 },
  { label: "120s", value: 120 },
];

const STYLE_PRESETS = [
  { label: "Classique", prompt: "Classical orchestral music, grand and majestic, cinematic orchestration" },
  { label: "Électronique", prompt: "Electronic dance music with synth pads, heavy bass, 128 BPM" },
  { label: "Jazz", prompt: "Smooth jazz, saxophone and piano, warm and intimate, slow tempo" },
  { label: "Rock", prompt: "Rock music with electric guitar, drums, energetic, driving beat" },
  { label: "Ambient", prompt: "Ambient atmospheric soundscape, calm, meditative, ethereal pads" },
  { label: "Lo-fi", prompt: "Lo-fi hip hop beat, relaxed, vinyl crackle, warm melody, study music" },
  { label: "Cinématique", prompt: "Cinematic orchestral soundtrack, epic, emotional, film score" },
  { label: "Acoustique", prompt: "Acoustic folk guitar, fingerpicking, warm, natural, intimate" },
];

function getMusicModels(models: string[]): string[] {
  return dedupeModelsByDirectory(models.filter(isMusicGenModel));
}

function firstMusicModel(models: string[]): string | undefined {
  return getMusicModels(models)[0];
}

// ── Main component ─────────────────────────────────────────────────────────

export function MusicGenPanel({ installedModels, onClose, inline }: Props) {
  // ── Core state
  const [prompt, setPrompt] = useState("");
  const [selectedModel, setSelectedModel] = useState<string>(() => firstMusicModel(installedModels) ?? "");

  // ── Generation parameters
  const [duration, setDuration] = useState(30);
  const [steps, setSteps] = useState(50);
  const [cfgScale, setCfgScale] = useState(3.0);
  const [negativePrompt, setNegativePrompt] = useState("");
  const [melodyFile, setMelodyFile] = useState<string | null>(null);
  const [melodyFileName, setMelodyFileName] = useState("");
  const [selectedStyle, setSelectedStyle] = useState<string | null>(null);

  // ── Generation results
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [generatedResult, setGeneratedResult] = useState<MusicJobResult | null>(null);
  const [taskProgress, setTaskProgress] = useState<{ progress: number; detail?: string; status?: string } | null>(null);

  // ── Refs
  const progressPollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const audioUrlRef = useRef<string | null>(null);

  useEffect(() => {
    return () => {
      if (audioUrlRef.current) URL.revokeObjectURL(audioUrlRef.current);
      if (progressPollRef.current) clearInterval(progressPollRef.current);
    };
  }, []);

  // ── Derived state
  const musicModels = useMemo(() => getMusicModels(installedModels), [installedModels]);
  const hasModels = musicModels.length > 0;
  const jobRunning = taskProgress?.status === "running" || false;

  // Keep selectedModel valid
  useEffect(() => {
    if (musicModels.length > 0 && !musicModels.includes(selectedModel)) {
      setSelectedModel(musicModels[0]);
    }
  }, [musicModels, selectedModel]);

  // ── Helpers ────────────────────────────────────────────────────────────

  async function handlePickMelody() {
    try {
      const path = await core.pickVoiceReference();
      if (!path) return;
      setMelodyFile(path);
      setMelodyFileName(path.split(/[/\\]/).pop() ?? path);
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error)?.message || "Impossible de choisir le fichier audio.");
    }
  }

  function clearMelody() {
    setMelodyFile(null);
    setMelodyFileName("");
  }

  function pollTask(
    taskId: string,
    setProgress: (p: { progress: number; detail?: string; status?: string } | null) => void,
    setResult: (r: MusicJobResult | null) => void
  ) {
    setProgress({ progress: 0, detail: "En attente…", status: "running" });
    const interval = setInterval(() => {
      const all = taskCenter.snapshot();
      const task = all.find((t) => t.id === taskId);
      if (task) {
        setProgress({ progress: task.progress ?? 0, detail: task.detail, status: task.status });
        if (task.status === "done" && task.resultAudioUrl) {
          setResult({ url: task.resultAudioUrl, path: task.resultAudioUrl });
          clearInterval(interval);
          setIsGenerating(false);
        }
        if (task.status === "error") {
          clearInterval(interval);
          setIsGenerating(false);
        }
      }
    }, 300);
    return interval;
  }

  // ── Generate ──────────────────────────────────────────────────────────

  async function handleGenerate() {
    const effectivePrompt = selectedStyle ?? prompt.trim();
    if (!effectivePrompt && !selectedStyle) {
      setError("Veuillez saisir un prompt ou choisir un style.");
      return;
    }
    setError(null);
    setIsGenerating(true);
    setGeneratedResult(null);
    setTaskProgress(null);

    try {
      const appInfo = await core.appInfo().catch(() => ({ data_dir: "/tmp" }));
      const outputDir = `${appInfo.data_dir}/generated_audio`;

      const taskId = startMusicGeneration({
        model: selectedModel,
        prompt: effectivePrompt,
        outputDir,
        duration,
        melodyReference: melodyFile ?? undefined,
        negativePrompt: negativePrompt.trim() || undefined,
        steps,
        cfgScale,
      });

      if (progressPollRef.current) clearInterval(progressPollRef.current);
      progressPollRef.current = pollTask(taskId, setTaskProgress, setGeneratedResult);
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error)?.message || "Échec de la génération musicale.");
      setIsGenerating(false);
    }
  }

  // ── When result arrives, stop loading state
  useEffect(() => {
    if (generatedResult) setIsGenerating(false);
  }, [generatedResult]);

  // ── Render ────────────────────────────────────────────────────────────

  const containerStyle: React.CSSProperties = inline
    ? { padding: 0, maxWidth: 900, margin: "0 auto" }
    : { padding: 24, maxWidth: 900, margin: "0 auto" };

  return (
    <div className={inline ? "" : "locaryn-card"} style={containerStyle}>
      {/* ── Header ── */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 20 }}>
        <div>
          <h3 style={{ margin: 0 }}>Génération de musique</h3>
          <p className="locaryn-field-hint" style={{ margin: "4px 0 0" }}>
            Créez de la musique et des sons à partir de texte avec des modèles locaux.
          </p>
        </div>
        {!inline && (
          <button type="button" className="locaryn-icon-btn" onClick={onClose} aria-label="Fermer">✕</button>
        )}
      </div>

      {/* ── Model selector ── */}
      {hasModels ? (
        <div style={{ marginBottom: 20 }}>
          <label className="locaryn-field-label">Modèle musical</label>
          <select
            className="locaryn-input"
            value={selectedModel}
            onChange={(e) => setSelectedModel(e.target.value)}
            disabled={jobRunning}
            style={{ width: "100%", fontSize: 13 }}
          >
            {musicModels.map((m) => {
              const name = m.split("/").pop() ?? m;
              return (
                <option key={m} value={m}>
                  {name}
                </option>
              );
            })}
          </select>
        </div>
      ) : (
        <div style={{
          padding: 24,
          borderRadius: 10,
          border: "1px dashed var(--border)",
          textAlign: "center",
          color: "var(--text-faint)",
          fontSize: 13,
          marginBottom: 20,
        }}>
          Aucun modèle de musique installé. Allez dans le Marketplace pour installer MusicGen, AudioLDM ou stable-audio.
        </div>
      )}

      {/* ── Prompt input ── */}
      <div className="locaryn-field" style={{ marginBottom: 16 }}>
        <label className="locaryn-field-label">
          Prompt
          <span style={{ fontWeight: 400, fontSize: 11, color: "var(--text-faint)", marginLeft: 8 }}>
            Décrivez la musique à générer
          </span>
        </label>
        <textarea
          className="locaryn-input"
          rows={3}
          placeholder="Ex: 'Lo-fi hip hop beat with warm piano and vinyl crackle, study music'"
          value={prompt}
          onChange={(e) => {
            setPrompt(e.target.value);
            setSelectedStyle(null);
          }}
          disabled={jobRunning}
          style={{ marginBottom: 10 }}
        />

        {/* ── Style presets ── */}
        <label className="locaryn-field-label" style={{ fontSize: 11, marginBottom: 6 }}>
          Ou choisissez un style prédéfini
        </label>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
          {STYLE_PRESETS.map((s) => (
            <button
              key={s.label}
              type="button"
              className={`locaryn-chip${selectedStyle === s.prompt ? " locaryn-chip-on" : ""}`}
              onClick={() => {
                setSelectedStyle(selectedStyle === s.prompt ? null : s.prompt);
                setPrompt("");
              }}
              disabled={jobRunning}
              style={{ fontSize: 11, padding: "2px 10px" }}
            >
              {s.label}
            </button>
          ))}
        </div>
      </div>

      {/* ── Advanced controls ── */}
      <details style={{ marginBottom: 16 }}>
        <summary style={{ fontSize: 12, fontWeight: 600, color: "var(--text-faint)", cursor: "pointer", userSelect: "none" }}>
          Paramètres avancés
        </summary>
        <div style={{ marginTop: 12, display: "grid", gridTemplateColumns: "1fr 1fr", gap: "12px 20px" }}>
          {/* Duration */}
          <div>
            <label className="locaryn-field-label" style={{ fontSize: 11, marginBottom: 4 }}>Durée</label>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
              {DURATION_PRESETS.map((d) => (
                <button
                  key={d.value}
                  type="button"
                  className={`locaryn-chip${duration === d.value ? " locaryn-chip-on" : ""}`}
                  onClick={() => setDuration(d.value)}
                  disabled={jobRunning}
                  style={{ fontSize: 11, padding: "2px 8px" }}
                >
                  {d.label}
                </button>
              ))}
            </div>
          </div>

          {/* Steps */}
          <div>
            <label className="locaryn-field-label" style={{ fontSize: 11, marginBottom: 4 }}>
              Étapes : {steps}
            </label>
            <input
              type="range"
              min={10}
              max={200}
              step={5}
              value={steps}
              onChange={(e) => setSteps(Number(e.target.value))}
              disabled={jobRunning}
              style={{ width: "100%" }}
            />
          </div>

          {/* CFG Scale */}
          <div>
            <label className="locaryn-field-label" style={{ fontSize: 11, marginBottom: 4 }}>
              Guidance (CFG) : {cfgScale.toFixed(1)}
            </label>
            <input
              type="range"
              min={1}
              max={10}
              step={0.5}
              value={cfgScale}
              onChange={(e) => setCfgScale(Number(e.target.value))}
              disabled={jobRunning}
              style={{ width: "100%" }}
            />
          </div>

          {/* Negative prompt */}
          <div>
            <label className="locaryn-field-label" style={{ fontSize: 11, marginBottom: 4 }}>Prompt négatif</label>
            <input
              type="text"
              className="locaryn-input"
              value={negativePrompt}
              onChange={(e) => setNegativePrompt(e.target.value)}
              placeholder="Éléments à éviter…"
              disabled={jobRunning}
              style={{ width: "100%", fontSize: 12, padding: "4px 8px" }}
            />
          </div>
        </div>

        {/* Melody reference (MusicGen Melody mode) */}
        <div style={{ marginTop: 12 }}>
          <label className="locaryn-field-label" style={{ fontSize: 11, marginBottom: 4 }}>
            Référence mélodique (optionnel)
          </label>
          {melodyFile ? (
            <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 12, color: "var(--text)" }}>
              <span>🎵 {melodyFileName}</span>
              <button type="button" className="locaryn-icon-btn" onClick={clearMelody} aria-label="Supprimer" style={{ fontSize: 14 }}>
                ✕
              </button>
              {/* melodyFile is a disk path: a webview cannot load it directly. */}
              <audio src={toMediaUrl(melodyFile)} controls style={{ height: 28, flex: 1 }} />
            </div>
          ) : (
            <button type="button" className="locaryn-btn-ghost" onClick={handlePickMelody} disabled={jobRunning} style={{ fontSize: 12 }}>
              + Importer un fichier audio
            </button>
          )}
        </div>
      </details>

      {/* ── Errors ── */}
      {error && (
        <div className="img-gen-error" style={{ marginBottom: 16 }}>
          <span>⚠️</span>
          <span>{error}</span>
        </div>
      )}

      {/* ── Result ── */}
      {generatedResult && (
        <div className="locaryn-field" style={{ marginBottom: 16 }}>
          <label className="locaryn-field-label">Musique générée</label>
          <audio src={generatedResult.url} controls style={{ width: "100%" }} />
        </div>
      )}

      {/* ── Progress + Actions ── */}
      <div className="locaryn-field-actions" style={{ justifyContent: "space-between" }}>
        {jobRunning && (
          <div style={{ flex: 1, marginRight: 12 }}>
            <div className="img-gen-progress-bar">
              <div className="img-gen-progress-fill" style={{ width: `${taskProgress?.progress ?? 0}%` }} />
            </div>
            <span className="locaryn-field-hint">{taskProgress?.detail ?? "Génération en cours…"}</span>
          </div>
        )}
        <div style={{ display: "flex", gap: 8, marginLeft: "auto" }}>
          {!inline && (
            <button type="button" className="locaryn-btn-ghost" onClick={onClose} disabled={isGenerating}>
              Fermer
            </button>
          )}
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={handleGenerate}
            disabled={(!prompt.trim() && !selectedStyle) || jobRunning || !hasModels}
          >
            {jobRunning ? "Génération…" : "🎵 Générer la musique"}
          </button>
        </div>
      </div>
    </div>
  );
}
