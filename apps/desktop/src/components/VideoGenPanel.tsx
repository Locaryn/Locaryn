import { useEffect, useMemo, useRef, useState } from "react";
import { core } from "../lib/core";
import { dedupeModelsByDirectory } from "../lib/modelList";
import { isVideoGenModel } from "../lib/modelRegistry";
import { taskCenter } from "../lib/taskCenter";
import { type VideoJobResult, startVideoGeneration } from "../lib/videoJobs";

// ── Types ──────────────────────────────────────────────────────────────────

type Props = {
  installedModels: string[];
  onClose: () => void;
  inline?: boolean;
};

const DURATION_PRESETS = [
  { label: "2s", value: 2 },
  { label: "4s", value: 4 },
  { label: "5s", value: 5 },
  { label: "6s", value: 6 },
];

function getVideoModels(models: string[]): string[] {
  return dedupeModelsByDirectory(models.filter(isVideoGenModel));
}

function firstVideoModel(models: string[]): string | undefined {
  return getVideoModels(models)[0];
}

// ── Main component ─────────────────────────────────────────────────────────

export function VideoGenPanel({ installedModels, onClose, inline }: Props) {
  // ── Core state
  const [prompt, setPrompt] = useState("");
  const [selectedModel, setSelectedModel] = useState<string>(
    () => firstVideoModel(installedModels) ?? "",
  );

  // ── Generation parameters
  const [mode, setMode] = useState<"t2v" | "i2v">("t2v");
  const [duration, setDuration] = useState(5);
  const [steps, setSteps] = useState(50);
  const [cfgScale, setCfgScale] = useState(7.0);
  const [negativePrompt, setNegativePrompt] = useState("");
  const [inputImage, setInputImage] = useState<string | null>(null);
  const [inputImageName, setInputImageName] = useState("");

  // ── Generation results
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [generatedResult, setGeneratedResult] = useState<VideoJobResult | null>(null);
  const [taskProgress, setTaskProgress] = useState<{
    progress: number;
    detail?: string;
    status?: string;
  } | null>(null);

  // ── Refs
  const progressPollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    return () => {
      if (progressPollRef.current) clearInterval(progressPollRef.current);
    };
  }, []);

  // ── Derived state
  const videoModels = useMemo(() => getVideoModels(installedModels), [installedModels]);
  const hasModels = videoModels.length > 0;
  const jobRunning = taskProgress?.status === "running" || false;

  // Keep selectedModel valid
  useEffect(() => {
    if (videoModels.length > 0 && !videoModels.includes(selectedModel)) {
      setSelectedModel(videoModels[0]);
    }
  }, [videoModels, selectedModel]);

  // Auto-switch to i2v mode when model supports it
  useEffect(() => {
    const hasI2V =
      selectedModel.toLowerCase().includes("i2v") || selectedModel.toLowerCase().includes("svd");
    if (hasI2V && mode === "t2v") setMode("i2v");
  }, [selectedModel, mode]);

  // ── Helpers ────────────────────────────────────────────────────────────

  function handlePickImage(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    setInputImageName(file.name);
    const reader = new FileReader();
    reader.onload = () => setInputImage(reader.result as string);
    reader.onerror = () => setError("Impossible de lire l'image source.");
    reader.readAsDataURL(file);
  }

  function clearImage() {
    setInputImage(null);
    setInputImageName("");
  }

  function pollTask(
    taskId: string,
    setProgress: (p: { progress: number; detail?: string; status?: string } | null) => void,
    setResult: (r: VideoJobResult | null) => void,
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
    if (!prompt.trim()) {
      setError("Veuillez saisir un prompt.");
      return;
    }
    if (mode === "i2v" && !inputImage) {
      setError("Veuillez sélectionner une image source.");
      return;
    }
    setError(null);
    setIsGenerating(true);
    setGeneratedResult(null);
    setTaskProgress(null);

    try {
      const appInfo = await core.appInfo().catch(() => ({ data_dir: "/tmp" }));
      const outputDir = `${appInfo.data_dir}/generated_videos`;

      const taskId = startVideoGeneration({
        model: selectedModel,
        prompt: prompt.trim(),
        outputDir,
        duration,
        inputImage: mode === "i2v" ? (inputImage ?? undefined) : undefined,
        negativePrompt: negativePrompt.trim() || undefined,
        steps,
        cfgScale,
      });

      if (progressPollRef.current) clearInterval(progressPollRef.current);
      progressPollRef.current = pollTask(taskId, setTaskProgress, setGeneratedResult);
    } catch (e) {
      setError(
        typeof e === "string" ? e : (e as Error)?.message || "Échec de la génération vidéo.",
      );
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
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 20,
        }}
      >
        <div>
          <h3 style={{ margin: 0 }}>Génération de vidéo</h3>
          <p className="locaryn-field-hint" style={{ margin: "4px 0 0" }}>
            Créez des vidéos à partir de texte ou d'images avec des modèles locaux.
          </p>
        </div>
        {!inline && (
          <button type="button" className="locaryn-icon-btn" onClick={onClose} aria-label="Fermer">
            ✕
          </button>
        )}
      </div>

      {/* ── Model selector ── */}
      {hasModels ? (
        <div style={{ marginBottom: 20 }}>
          <label className="locaryn-field-label">Modèle vidéo</label>
          <select
            className="locaryn-input"
            value={selectedModel}
            onChange={(e) => setSelectedModel(e.target.value)}
            disabled={jobRunning}
            style={{ width: "100%", fontSize: 13 }}
          >
            {videoModels.map((m) => {
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
        <div
          style={{
            padding: 24,
            borderRadius: 10,
            border: "1px dashed var(--border)",
            textAlign: "center",
            color: "var(--text-faint)",
            fontSize: 13,
            marginBottom: 20,
          }}
        >
          Aucun modèle vidéo installé. Allez dans le Marketplace pour installer Wan 2.1, LTX Video
          ou Stable Video Diffusion.
        </div>
      )}

      {/* ── Mode tabs ── */}
      <div style={{ display: "flex", borderBottom: "1px solid var(--border)", marginBottom: 14 }}>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "t2v"}
          disabled={jobRunning}
          onClick={() => setMode("t2v")}
          style={{
            flex: 1,
            padding: "10px 8px",
            border: "none",
            borderBottom: mode === "t2v" ? "2px solid var(--accent)" : "2px solid transparent",
            background: mode === "t2v" ? "rgba(100,150,255,0.04)" : "transparent",
            color: mode === "t2v" ? "var(--text)" : "var(--text-faint)",
            fontSize: 12,
            fontWeight: mode === "t2v" ? 600 : 400,
            cursor: jobRunning ? "default" : "pointer",
          }}
        >
          ✨ Texte → Vidéo
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "i2v"}
          disabled={jobRunning}
          onClick={() => setMode("i2v")}
          style={{
            flex: 1,
            padding: "10px 8px",
            border: "none",
            borderBottom: mode === "i2v" ? "2px solid var(--accent)" : "2px solid transparent",
            background: mode === "i2v" ? "rgba(100,150,255,0.04)" : "transparent",
            color: mode === "i2v" ? "var(--text)" : "var(--text-faint)",
            fontSize: 12,
            fontWeight: mode === "i2v" ? 600 : 400,
            cursor: jobRunning ? "default" : "pointer",
          }}
        >
          🖼️ Image → Vidéo
        </button>
      </div>

      {/* ── Image source (i2v) ── */}
      {mode === "i2v" && (
        <div className="locaryn-field" style={{ marginBottom: 16 }}>
          <label className="locaryn-field-label">Image source</label>
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            style={{ display: "none" }}
            onChange={handlePickImage}
          />
          {inputImage ? (
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ fontSize: 12, color: "var(--text)" }}>🖼️ {inputImageName}</span>
              <button
                type="button"
                className="locaryn-icon-btn"
                onClick={clearImage}
                aria-label="Supprimer"
              >
                ✕
              </button>
            </div>
          ) : (
            <button
              type="button"
              className="locaryn-btn-ghost"
              onClick={() => fileInputRef.current?.click()}
              disabled={jobRunning}
              style={{ fontSize: 12 }}
            >
              + Choisir une image
            </button>
          )}
        </div>
      )}

      {/* ── Prompt input ── */}
      <div className="locaryn-field" style={{ marginBottom: 16 }}>
        <label className="locaryn-field-label">
          Prompt
          <span
            style={{ fontWeight: 400, fontSize: 11, color: "var(--text-faint)", marginLeft: 8 }}
          >
            Décrivez la vidéo à générer
          </span>
        </label>
        <textarea
          className="locaryn-input"
          rows={3}
          placeholder="Ex: 'A cinematic shot of a dolphin swimming through neon-lit cyberpunk canals, slow motion'"
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          disabled={jobRunning}
        />
      </div>

      {/* ── Advanced controls ── */}
      <details style={{ marginBottom: 16 }}>
        <summary
          style={{
            fontSize: 12,
            fontWeight: 600,
            color: "var(--text-faint)",
            cursor: "pointer",
            userSelect: "none",
          }}
        >
          Paramètres avancés
        </summary>
        <div
          style={{
            marginTop: 12,
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: "12px 20px",
          }}
        >
          {/* Duration */}
          <div>
            <label className="locaryn-field-label" style={{ fontSize: 11, marginBottom: 4 }}>
              Durée
            </label>
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
              max={100}
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
              max={20}
              step={0.5}
              value={cfgScale}
              onChange={(e) => setCfgScale(Number(e.target.value))}
              disabled={jobRunning}
              style={{ width: "100%" }}
            />
          </div>

          {/* Negative prompt */}
          <div>
            <label className="locaryn-field-label" style={{ fontSize: 11, marginBottom: 4 }}>
              Prompt négatif
            </label>
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
          <label className="locaryn-field-label">Vidéo générée</label>
          <video
            src={generatedResult.url}
            controls
            style={{ width: "100%", borderRadius: 8, maxHeight: 400 }}
          />
        </div>
      )}

      {/* ── Progress + Actions ── */}
      <div className="locaryn-field-actions" style={{ justifyContent: "space-between" }}>
        {jobRunning && (
          <div style={{ flex: 1, marginRight: 12 }}>
            <div className="img-gen-progress-bar">
              <div
                className="img-gen-progress-fill"
                style={{ width: `${taskProgress?.progress ?? 0}%` }}
              />
            </div>
            <span className="locaryn-field-hint">
              {taskProgress?.detail ?? "Génération en cours…"}
            </span>
          </div>
        )}
        <div style={{ display: "flex", gap: 8, marginLeft: "auto" }}>
          {!inline && (
            <button
              type="button"
              className="locaryn-btn-ghost"
              onClick={onClose}
              disabled={isGenerating}
            >
              Fermer
            </button>
          )}
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={handleGenerate}
            disabled={!prompt.trim() || jobRunning || !hasModels || (mode === "i2v" && !inputImage)}
          >
            {jobRunning ? "Génération…" : "🎬 Générer la vidéo"}
          </button>
        </div>
      </div>
    </div>
  );
}
