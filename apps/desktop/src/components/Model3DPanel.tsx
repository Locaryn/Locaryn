import { Icon } from "@locaryn/ui-core";
import { useEffect, useMemo, useRef, useState } from "react";
import { core } from "../lib/core";
import { type Model3DJobResult, startModel3DGeneration } from "../lib/model3dJobs";
import { dedupeModelsByDirectory } from "../lib/modelList";
import { isModel3DGenModel } from "../lib/modelRegistry";
import { taskCenter } from "../lib/taskCenter";

// ── Types ──────────────────────────────────────────────────────────────────

type Props = {
  installedModels: string[];
  onClose: () => void;
  inline?: boolean;
};

function get3DModels(models: string[]): string[] {
  return dedupeModelsByDirectory(models.filter(isModel3DGenModel));
}

function first3DModel(models: string[]): string | undefined {
  return get3DModels(models)[0];
}

// ── Main component ─────────────────────────────────────────────────────────

export function Model3DPanel({ installedModels, onClose, inline }: Props) {
  // ── Core state
  const [prompt, setPrompt] = useState("");
  const [selectedModel, setSelectedModel] = useState<string>(
    () => first3DModel(installedModels) ?? "",
  );

  // ── Generation parameters
  const [mode, setMode] = useState<"t2m" | "i2m">("t2m");
  const [steps, setSteps] = useState(50);
  const [cfgScale, setCfgScale] = useState(7.0);
  const [negativePrompt, setNegativePrompt] = useState("");
  const [outputFormat, setOutputFormat] = useState<string>("obj");
  const [inputImage, setInputImage] = useState<string | null>(null);
  const [inputImageName, setInputImageName] = useState("");

  // ── Generation results
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [generatedResult, setGeneratedResult] = useState<Model3DJobResult | null>(null);
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
  const threeDModels = useMemo(() => get3DModels(installedModels), [installedModels]);
  const hasModels = threeDModels.length > 0;
  const jobRunning = taskProgress?.status === "running" || false;

  // Keep selectedModel valid
  useEffect(() => {
    if (threeDModels.length > 0 && !threeDModels.includes(selectedModel)) {
      setSelectedModel(threeDModels[0]);
    }
  }, [threeDModels, selectedModel]);

  // Auto-switch to i2m mode when model supports it
  useEffect(() => {
    const hasI2M =
      selectedModel.toLowerCase().includes("tripo") || selectedModel.toLowerCase().includes("zero");
    if (hasI2M && mode === "t2m") setMode("i2m");
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
    setResult: (r: Model3DJobResult | null) => void,
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
    if (mode === "i2m" && !inputImage) {
      setError("Veuillez sélectionner une image source.");
      return;
    }
    setError(null);
    setIsGenerating(true);
    setGeneratedResult(null);
    setTaskProgress(null);

    try {
      const dataDir = await core
        .appInfo()
        .then((info) => info.data_dir)
        .catch(() => "/tmp");
      const outputDir = `${dataDir}/generated_3d`;

      const taskId = startModel3DGeneration({
        model: selectedModel,
        prompt: prompt.trim(),
        outputDir,
        inputImage: mode === "i2m" ? (inputImage ?? undefined) : undefined,
        negativePrompt: negativePrompt.trim() || undefined,
        steps,
        cfgScale,
        format: outputFormat,
      });

      if (progressPollRef.current) clearInterval(progressPollRef.current);
      progressPollRef.current = pollTask(taskId, setTaskProgress, setGeneratedResult);
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error)?.message || "Échec de la génération 3D.");
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
          <h3 style={{ margin: 0 }}>Génération 3D</h3>
          <p className="locaryn-field-hint" style={{ margin: "4px 0 0" }}>
            Créez des maillages, modèles et actifs 3D à partir de texte ou d'images.
          </p>
        </div>
        {!inline && (
          <button type="button" className="locaryn-icon-btn" onClick={onClose} aria-label="Fermer">
            <Icon name="close" size={16} />
          </button>
        )}
      </div>

      {/* ── Model selector ── */}
      {hasModels ? (
        <div style={{ marginBottom: 20 }}>
          <label className="locaryn-field-label" htmlFor="model3d-model">
            Modèle 3D
          </label>
          <select
            id="model3d-model"
            className="locaryn-input"
            value={selectedModel}
            onChange={(e) => setSelectedModel(e.target.value)}
            disabled={jobRunning}
            style={{ width: "100%", fontSize: 13 }}
          >
            {threeDModels.map((m) => {
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
          Aucun modèle 3D installé. Allez dans le Marketplace pour installer Shape-E, Point-E,
          TripoSR ou Zero-1-to-3.
        </div>
      )}

      {/* ── Mode tabs ── */}
      <div style={{ display: "flex", borderBottom: "1px solid var(--border)", marginBottom: 14 }}>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "t2m"}
          disabled={jobRunning}
          onClick={() => setMode("t2m")}
          style={{
            flex: 1,
            padding: "10px 8px",
            border: "none",
            borderBottom: mode === "t2m" ? "2px solid var(--accent)" : "2px solid transparent",
            background: mode === "t2m" ? "rgba(100,150,255,0.04)" : "transparent",
            color: mode === "t2m" ? "var(--text)" : "var(--text-faint)",
            fontSize: 12,
            fontWeight: mode === "t2m" ? 600 : 400,
            cursor: jobRunning ? "default" : "pointer",
          }}
        >
          <Icon name="star" size={15} /> Texte → 3D
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "i2m"}
          disabled={jobRunning}
          onClick={() => setMode("i2m")}
          style={{
            flex: 1,
            padding: "10px 8px",
            border: "none",
            borderBottom: mode === "i2m" ? "2px solid var(--accent)" : "2px solid transparent",
            background: mode === "i2m" ? "rgba(100,150,255,0.04)" : "transparent",
            color: mode === "i2m" ? "var(--text)" : "var(--text-faint)",
            fontSize: 12,
            fontWeight: mode === "i2m" ? 600 : 400,
            cursor: jobRunning ? "default" : "pointer",
          }}
        >
          <Icon name="image" size={15} /> Image → 3D
        </button>
      </div>

      {/* ── Image source (i2m) ── */}
      {mode === "i2m" && (
        <div className="locaryn-field" style={{ marginBottom: 16 }}>
          {/* Le seul contrôle visible est le bouton ci-dessous ; étiqueter l'input
              caché déclencherait le sélecteur de fichiers au clic sur le titre. */}
          <div className="locaryn-field-label">Image source</div>
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            style={{ display: "none" }}
            onChange={handlePickImage}
          />
          {inputImage ? (
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ fontSize: 12, color: "var(--text)" }}>
                <Icon name="image" size={13} /> {inputImageName}
              </span>
              <button
                type="button"
                className="locaryn-icon-btn"
                onClick={clearImage}
                aria-label="Supprimer"
              >
                <Icon name="close" size={16} />
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
        <label className="locaryn-field-label" htmlFor="model3d-prompt">
          Prompt
          <span
            style={{ fontWeight: 400, fontSize: 11, color: "var(--text-faint)", marginLeft: 8 }}
          >
            Décrivez le modèle 3D à générer
          </span>
        </label>
        <textarea
          id="model3d-prompt"
          className="locaryn-input"
          rows={3}
          placeholder='Ex: "An ornate wooden chair with carved armrests, baroque style"'
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
          {/* Steps */}
          <div>
            <label
              className="locaryn-field-label"
              htmlFor="model3d-steps"
              style={{ fontSize: 11, marginBottom: 4 }}
            >
              Étapes : {steps}
            </label>
            <input
              id="model3d-steps"
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
            <label
              htmlFor="m3d-prompt"
              className="locaryn-field-label"
              style={{ fontSize: 11, marginBottom: 4 }}
            >
              Guidance (CFG) : {cfgScale.toFixed(1)}
            </label>
            <input
              id="m3d-prompt"
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
            <label
              htmlFor="m3d-negative"
              className="locaryn-field-label"
              style={{ fontSize: 11, marginBottom: 4 }}
            >
              Prompt négatif
            </label>
            <input
              id="m3d-negative"
              type="text"
              className="locaryn-input"
              value={negativePrompt}
              onChange={(e) => setNegativePrompt(e.target.value)}
              placeholder="Éléments à éviter…"
              disabled={jobRunning}
              style={{ width: "100%", fontSize: 12, padding: "4px 8px" }}
            />
          </div>

          {/* Output format */}
          <div>
            <label
              htmlFor="m3d-steps"
              className="locaryn-field-label"
              style={{ fontSize: 11, marginBottom: 4 }}
            >
              Format de sortie
            </label>
            <select
              id="m3d-steps"
              className="locaryn-input"
              value={outputFormat}
              onChange={(e) => setOutputFormat(e.target.value)}
              disabled={jobRunning}
              style={{ width: "100%", fontSize: 12, padding: "4px 8px" }}
            >
              <option value="obj">Wavefront .obj</option>
              <option value="glb">glTF Binary .glb</option>
              <option value="ply">Stanford .ply</option>
            </select>
          </div>
        </div>
      </details>

      {/* ── Errors ── */}
      {error && (
        <div className="img-gen-error" style={{ marginBottom: 16 }}>
          <Icon name="warning" size={15} />
          <span>{error}</span>
        </div>
      )}

      {/* ── Result ── */}
      {generatedResult && (
        <div className="locaryn-field" style={{ marginBottom: 16 }}>
          <div className="locaryn-field-label">Modèle 3D généré</div>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              padding: 16,
              borderRadius: 10,
              border: "1px solid var(--border)",
              background: "var(--bg-alt)",
            }}
          >
            <span style={{ fontSize: 12, color: "var(--text-faint)" }}>
              <Icon name="models" size={15} /> Fichier généré :{" "}
              {generatedResult.path.split("/").pop() || "modèle 3D"}
            </span>
            <div style={{ display: "flex", gap: 8 }}>
              <a
                href={generatedResult.url}
                download
                className="locaryn-btn-primary"
                style={{ fontSize: 12, textDecoration: "none" }}
              >
                Télécharger
              </a>
            </div>
          </div>
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
            disabled={!prompt.trim() || jobRunning || !hasModels || (mode === "i2m" && !inputImage)}
          >
            {jobRunning ? "Génération…" : "Générer le modèle 3D"}
          </button>
        </div>
      </div>
    </div>
  );
}
