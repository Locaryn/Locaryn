import { useRef, useState, useEffect, useMemo } from "react";
import { core, type VramMode } from "../lib/core";
import { ResponsibilityGate } from "./ResponsibilityGate";
import { startImageGeneration, getActiveImageJob } from "../lib/imageJobs";
import { useTasks } from "../lib/taskCenter";
import { IMAGE_GEN_MODELS, findInstalledImageGenModel } from "../lib/modelRegistry";
import { classifyModel, nsfwReason } from "../lib/modelSafety";
import { convertFileSrc } from "@tauri-apps/api/core";
import { estimateImageGenerationDuration, formatEstimatedDuration } from "../lib/durationEstimator";
import { dedupeModelsByDirectory } from "../lib/modelList";

// ── Types ────────────────────────────────────────────────────────────────────

type Props = {
  installedModels: string[];
  onClose: () => void;
  onImageGenerated?: (imagePath: string) => void;
  onInstallRequested?: (modelTag: string, consent?: boolean) => Promise<void>;
  /** Chat that requested the generation, so the result lands in it. */
  sessionId?: string | null;
  /** Resolution requested through a slash argument (e.g. "/image max"). */
  forcedSize?: number | null;
  /** Render inline without the modal overlay (used in Studio). */
  inline?: boolean;
};

// ── Animated shimmer placeholder shown while generating ───────────────────────

function GeneratingPlaceholder({ mode }: { mode: "txt2img" | "img2img" }) {
  return (
    <div className="img-gen-placeholder">
      <div className="img-gen-shimmer" />
      <div className="img-gen-pulse-ring" />
      <div className="img-gen-pulse-ring img-gen-pulse-ring-2" />
      <div className="img-gen-generating-label">
        <span className="img-gen-spinner">◌</span>
        {mode === "img2img" ? "Transformation en cours…" : "Génération en cours…"}
      </div>
      <div className="img-gen-sub-label">sd.exe traite le prompt sur ton GPU</div>
    </div>
  );
}

// ── Animated step indicator ───────────────────────────────────────────────────

function StepDots({ elapsed }: { elapsed: number }) {
  // sd.cpp sampling steps — typical run ~15-120s
  const steps = [
    { label: "Initialisation", doneAt: 2 },
    { label: "Chargement modèle", doneAt: 8 },
    { label: "Encoding prompt", doneAt: 12 },
    { label: "Débruitage (steps)", doneAt: 80 },
    { label: "Décodage image", doneAt: 95 },
    { label: "Écriture fichier", doneAt: 100 },
  ];
  return (
    <div className="img-gen-steps">
      {steps.map((s) => {
        const done = elapsed >= s.doneAt;
        const active = !done && elapsed >= (steps[steps.indexOf(s) - 1]?.doneAt ?? 0);
        return (
          <div key={s.label} className={`img-gen-step${done ? " img-gen-step-done" : active ? " img-gen-step-active" : ""}`}>
            <span className="img-gen-step-dot">{done ? "✓" : active ? "◌" : "○"}</span>
            <span>{s.label}</span>
          </div>
        );
      })}
    </div>
  );
}

/** Sentinel dropdown value for the uncensored (abliterated-encoder) variant. */
const HERETIC_VALUE = "__heretic__";

function isUncensoredName(name: string): boolean {
  const risk = classifyModel(name).risk;
  return risk === "uncensored" || risk === "nsfw";
}

// ── Main component ────────────────────────────────────────────────────────────

export function ImageGenPanel({ installedModels, onClose, onImageGenerated, onInstallRequested, sessionId, forcedSize, inline }: Props) {
  // Reopening while a generation is RUNNING restores that job's context (prompt,
  // source image, mode) and shows its live progress. A finished job must NOT be
  // restored: it would prefill an old prompt and show its old image, making a
  // fresh request look like it was ignored.
  const active = getActiveImageJob();
  const restored = active?.running ? active : null;
  const [mode, setMode] = useState<"txt2img" | "img2img">(restored?.mode ?? "txt2img");
  const [prompt, setPrompt] = useState(restored?.params.prompt ?? "");
  const [negPrompt, setNegPrompt] = useState(restored?.params.negativePrompt ?? "");
  const [sourceImagePath, setSourceImagePath] = useState<string | null>(restored?.params.inputImage ?? null);
  const [sourceImagePreview, setSourceImagePreview] = useState<string | null>(restored?.params.inputImage ?? null);
  const [isGenerating, setIsGenerating] = useState(false);
  const [isInstalling, setIsInstalling] = useState(false);
  const [localInstalledModels, setLocalInstalledModels] = useState<string[]>(installedModels);
  const [generatedImagePath, setGeneratedImagePath] = useState<string | null>(null);
  const [isSimulated, setIsSimulated] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [steps, setSteps] = useState(20);
  const [cfg_scale, setCfgScale] = useState(7);
  const [width, setWidth] = useState(512);
  const [height, setHeight] = useState(512);
  const [vramMode, setVramMode] = useState<VramMode>(restored?.params.vramMode ?? "auto");
  const [uncensored, setUncensored] = useState(restored?.params.uncensored ?? false);
  const [gateOpen, setGateOpen] = useState(false);
  const [nsfwGateOpen, setNsfwGateOpen] = useState(false);
  const [nsfwAccepted, setNsfwAccepted] = useState(false);
  const [nsfwInstallPending, setNsfwInstallPending] = useState(false);

  /** Z-Image checkpoint that the abliterated encoder can be paired with, if the
   *  encoder is installed — enables the "Sans limite (Heretic)" dropdown entry. */
  const [hereticBase, setHereticBase] = useState<string | null>(null);

  // Live status of the restored/active job (progress bar + result thumbnail).
  const tasks = useTasks();
  const activeTask = restored ? tasks.find((t) => t.id === restored.taskId) : undefined;
  const jobRunning = activeTask?.status === "running";

  const fileInputRef = useRef<HTMLInputElement>(null);
  const elapsedRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const startTimeRef = useRef<number>(0);

  // The dropdown is fed by the dedicated diffusion-checkpoint list — NOT the
  // chat-model list. Aux files (VAE, text-encoder LLM, mmproj, test images)
  // never appear here; generate_image resolves those dependencies itself.
  useEffect(() => {
    let cancelled = false;
    core
      .listImageModels()
      .then((list) => {
        if (!cancelled) setLocalInstalledModels(list);
      })
      .catch(() => {
        // Fall back to the prop (older backend) if the command is missing.
        if (!cancelled) setLocalInstalledModels(installedModels);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // A slash argument ("/image max") wins over the saved default.
  useEffect(() => {
    if (forcedSize) {
      setWidth(forcedSize);
      setHeight(forcedSize);
    }
  }, [forcedSize]);

  // Elapsed timer during generation
  useEffect(() => {
    if (isGenerating) {
      startTimeRef.current = Date.now();
      setElapsed(0);
      elapsedRef.current = setInterval(() => {
        setElapsed(Math.round((Date.now() - startTimeRef.current) / 1000));
      }, 500);
    } else {
      if (elapsedRef.current) clearInterval(elapsedRef.current);
    }
    return () => { if (elapsedRef.current) clearInterval(elapsedRef.current); };
  }, [isGenerating]);

  const dedupedLocalInstalledModels = useMemo(
    () => dedupeModelsByDirectory(localInstalledModels),
    [localInstalledModels]
  );

  const installedOptions = useMemo(() => {
    const opts: { label: string; value: string }[] = [];
    const added = new Set<string>();

    for (const f of IMAGE_GEN_MODELS) {
      for (const v of f.variants) {
        const fileName = v.tag.startsWith("http") ? v.tag.split("/").pop()! : v.tag;
        const isMatch = dedupedLocalInstalledModels.some((m) => {
          const mLower = m.toLowerCase();
          const fLower = f.id.toLowerCase();
          const vLower = fileName.toLowerCase();
          const tagLower = v.tag.toLowerCase();
          const mClean = mLower.replace(/^x\//, "").replace(/:latest$/, "");
          return (
            m === fileName ||
            m === v.tag ||
            mLower.includes(fLower) ||
            mLower.includes(vLower) ||
            (mClean && vLower.includes(mClean)) ||
            tagLower.includes(mLower)
          );
        });

        const uncensored = !!f.uncensored || isUncensoredName(v.tag);
        if (isMatch && !added.has(v.tag)) {
          const padlock = uncensored ? "🔓 " : "";
          opts.push({ label: `${padlock}${f.name} (${v.size})`, value: v.tag });
          added.add(v.tag);
          added.add(fileName);
          added.add(fileName.toLowerCase());
          // Also mark matched raw model names so they don't appear again
          for (const m of dedupedLocalInstalledModels) {
            const mLower = m.toLowerCase();
            if (mLower.includes(f.id.toLowerCase()) || mLower.includes(fileName.toLowerCase())) {
              added.add(m);
            }
          }
        }
      }
    }

    // Remaining local checkpoints not matched to a catalogue entry. The list
    // already contains only real diffusion weights (list_image_models), so no
    // extra filtering — just avoid duplicates.
    for (const m of dedupedLocalInstalledModels) {
      if (added.has(m)) continue;
      const padlock = isUncensoredName(m) ? "🔓 " : "";
      opts.push({ label: `${padlock}${m} (Modèle local)`, value: m });
      added.add(m);
    }

    return opts;
  }, [dedupedLocalInstalledModels]);

  const installedImageGenTag = findInstalledImageGenModel(dedupedLocalInstalledModels);
  const [selectedModel, setSelectedModel] = useState<string>(
    restored?.params.model ?? installedOptions[0]?.value ?? (installedImageGenTag ?? (IMAGE_GEN_MODELS[0]?.variants[0]?.tag ?? ""))
  );

  useEffect(() => {
    if (installedOptions.length > 0 && !installedOptions.some((o: { label: string; value: string }) => o.value === selectedModel)) {
      setSelectedModel(installedOptions[0].value);
    }
  }, [installedOptions, selectedModel]);

  // Reset per-model NSFW consent when the selected checkpoint changes.
  useEffect(() => {
    setNsfwAccepted(false);
  }, [selectedModel]);

  // Offer the uncensored variant only when BOTH pieces are present: a Z-Image
  // diffusion checkpoint and an abliterated text encoder.
  useEffect(() => {
    let cancelled = false;
    const zBase = installedOptions.find((o) => /z[_-]?image/i.test(o.value))?.value;
    if (!zBase) {
      setHereticBase(null);
      return;
    }
    core.hasAbliteratedEncoder()
      .then((has) => { if (!cancelled) setHereticBase(has ? zBase : null); })
      .catch(() => { if (!cancelled) setHereticBase(null); });
    return () => { cancelled = true; };
  }, [installedOptions]);

  // Pick model-appropriate defaults:
  // - Turbo/distilled (Z-Image Turbo, SDXL-Turbo, FLUX-schnell): ~8 steps, CFG 1.
  // - FLOW-matching base (Z-Image, FLUX): CFG 1 (guidance internal — a high CFG
  //   turns the output into noise), but more steps than the distilled turbo.
  useEffect(() => {
    const t = selectedModel.toLowerCase();
    const isTurbo = t.includes("turbo") || t.includes("schnell");
    const isFlow = t.includes("z_image") || t.includes("z-image") || t.includes("flux");
    if (isTurbo) {
      setSteps(8);
      setCfgScale(1);
    } else if (isFlow) {
      setSteps(20);
      setCfgScale(1);
    }
  }, [selectedModel]);

  const isSelectedInstalled = installedOptions.some((o: { label: string; value: string }) => o.value === selectedModel) || localInstalledModels.length > 0;
  const selectedNsfw = classifyModel(selectedModel).risk !== "safe";

  const estimatedDuration = useMemo(() => {
    return estimateImageGenerationDuration({
      model: selectedModel,
      vramMode,
      mode,
      steps,
      width,
      height,
    });
  }, [selectedModel, vramMode, mode, steps, width, height]);

  function handlePickSourceImage(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    setSourceImagePreview(URL.createObjectURL(file));
    // A webview <input type=file> exposes no real disk path, so pass the image
    // as a base64 data URL — the backend decodes it to a temp file for sd.exe.
    const reader = new FileReader();
    reader.onload = () => setSourceImagePath(reader.result as string);
    reader.onerror = () => setError("Impossible de lire l'image source.");
    reader.readAsDataURL(file);
  }

  async function handleInstall(consentOverride = false) {
    if (selectedNsfw && !nsfwAccepted && !consentOverride) {
      setNsfwInstallPending(true);
      setNsfwGateOpen(true);
      return;
    }
    setIsInstalling(true);
    setError(null);
    try {
      if (onInstallRequested) {
        await onInstallRequested(selectedModel, true);
      } else {
        const providers = await core.listProviders();
        const active = providers.find((p) => p.is_active) ?? providers[0];
        if (active) await core.pullModel(active.endpoint, selectedModel, undefined, undefined, true);
      }
      const fileName = selectedModel.startsWith("http")
        ? selectedModel.split("/").pop()!
        : selectedModel;
      setLocalInstalledModels((prev) => [...new Set([...prev, fileName, selectedModel])]);
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error)?.message || "Échec du téléchargement du modèle.");
    } finally {
      setIsInstalling(false);
    }
  }

  // Background generation: kick the job off, then close the popup. Progress
  // shows in the notification center and the finished image lands in the chat.
  async function handleGenerate() {
    if (!prompt.trim() || isGenerating) return;
    if (selectedNsfw && !nsfwAccepted) {
      setNsfwGateOpen(true);
      return;
    }
    if (mode === "img2img" && !sourceImagePath) {
      setError("Veuillez sélectionner une image source à éditer.");
      return;
    }
    setError(null);
    const appInfo = await core.appInfo().catch(() => ({ data_dir: "C:/Users/Public" }));
    const outputDir = `${appInfo.data_dir}/generated_images`;
    startImageGeneration({
      model: selectedModel,
      prompt,
      outputDir,
      inputImage: mode === "img2img" ? (sourceImagePath ?? undefined) : undefined,
      negativePrompt: negPrompt || undefined,
      steps,
      cfgScale: cfg_scale,
      width,
      height,
      vramMode,
      uncensored,
      consent: nsfwAccepted,
      sessionId,
    });
    onClose();
  }

  const panelContent = (
    <div className={inline ? "img-gen-inline" : "img-gen-modal"}>

      {/* ── Header ── */}
      <div className="img-gen-header">
        <div className="img-gen-title-wrap">
          <span className="img-gen-icon">🎨</span>
          <div>
            <div className="img-gen-title">Génération d'Images IA</div>
            <div className="img-gen-subtitle">Local · sd.cpp · Aucune API externe</div>
          </div>
        </div>
        {!inline && (
          <button
            type="button"
            className="img-gen-close"
            onClick={onClose}
            disabled={isInstalling}
            aria-label="Fermer"
          >
            ✕
          </button>
        )}
      </div>

        {/* ── Legal strip ── */}
        <div className="img-gen-legal">
          <span>⚖️</span>
          <span>Les modèles s'exécutent localement. L'utilisateur est seul responsable des contenus générés.</span>
        </div>

        {/* ── Mode tabs ── */}
        <div className="img-gen-tabs">
          <button
            type="button"
            className={`img-gen-tab${mode === "txt2img" ? " img-gen-tab-active" : ""}`}
            onClick={() => setMode("txt2img")}
            disabled={isGenerating}
          >
            ✨ Texte → Image
          </button>
          <button
            type="button"
            className={`img-gen-tab${mode === "img2img" ? " img-gen-tab-active" : ""}`}
            onClick={() => setMode("img2img")}
            disabled={isGenerating}
          >
            🖼️ Image → Image
          </button>
        </div>

        {/* ── Model selector ── */}
        <div className="img-gen-field">
          <div className="img-gen-field-row">
            <label className="img-gen-label">Modèle</label>
            <span className="img-gen-model-count">
              {installedOptions.length === 0 ? "Aucun installé" : `${installedOptions.length} disponible${installedOptions.length > 1 ? "s" : ""}`}
            </span>
          </div>
          {selectedNsfw && (
            <div className="img-gen-error" style={{ marginBottom: 8 }}>
              <span>🔞</span>
              <span>{nsfwReason(selectedModel)} — le mode responsabilité sera demandé avant génération.</span>
            </div>
          )}
          {installedOptions.length === 0 ? (
            <div className="img-gen-no-model">
              <span>⚠️</span>
              <span>Aucun modèle d'image installé. Allez dans le Marketplace pour en installer un.</span>
              <button
                type="button"
                className="img-gen-install-btn"
                onClick={() => handleInstall()}
                disabled={isInstalling}
              >
                {isInstalling ? "Installation…" : "⬇ Installer"}
              </button>
            </div>
          ) : (
            <select
              className="locaryn-select"
              value={uncensored ? HERETIC_VALUE : selectedModel}
              disabled={jobRunning || installedOptions.length === 0}
              onChange={(e) => {
                if (e.target.value === HERETIC_VALUE) {
                  // Uncensored variant: same diffusion checkpoint, abliterated
                  // text encoder — gated by the responsibility prompt.
                  setGateOpen(true);
                } else {
                  setUncensored(false);
                  setSelectedModel(e.target.value);
                }
              }}
            >
              {installedOptions.map((o: { label: string; value: string }) => (
                <option key={o.value} value={o.value}>{o.label}</option>
              ))}
              {hereticBase && (
                <option value={HERETIC_VALUE}>🔓 Z-Image Turbo — Sans limite (Heretic)</option>
              )}
            </select>
          )}
        </div>

        {/* ── Source image (img2img) ── */}
        {mode === "img2img" && (
          <div className="img-gen-field">
            <label className="img-gen-label">Image source à éditer</label>
            <input ref={fileInputRef} type="file" accept="image/*" style={{ display: "none" }} onChange={handlePickSourceImage} />
            <div
              className={`img-gen-dropzone${sourceImagePreview ? " img-gen-dropzone-filled" : ""}`}
              onClick={() => !isGenerating && fileInputRef.current?.click()}
              onDragOver={(e) => { e.preventDefault(); e.stopPropagation(); }}
              onDrop={(e) => {
                e.preventDefault();
                e.stopPropagation();
                // Accept drops from gallery: path in custom type, or URL fallback
                const path = e.dataTransfer.getData("text/x-locaryn-image-path");
                const url = e.dataTransfer.getData("text/plain");
                if (path) {
                  setSourceImagePath(path);
                  setSourceImagePreview(url || path);
                } else if (url && url.startsWith("data:")) {
                  setSourceImagePath(url);
                  setSourceImagePreview(url);
                } else if (url) {
                  // asset:// or http URL — use directly as preview
                  setSourceImagePath(url);
                  setSourceImagePreview(url);
                }
              }}
            >
              {sourceImagePreview ? (
                <div className="img-gen-preview-wrap">
                  <img src={sourceImagePreview} alt="Source" className="img-gen-preview-img" />
                  <button
                    type="button"
                    className="img-gen-remove-btn"
                    onClick={(e) => { e.stopPropagation(); setSourceImagePath(null); setSourceImagePreview(null); }}
                  >✕</button>
                </div>
              ) : (
                <>
                  <span className="img-gen-drop-icon">🖼️</span>
                  <span className="img-gen-drop-text">Cliquez pour choisir une image</span>
                  <span className="img-gen-drop-hint">PNG, JPG, WEBP</span>
                </>
              )}
            </div>
          </div>
        )}

        {/* ── Prompt ── */}
        <div className="img-gen-field">
          <label className="img-gen-label">
            {mode === "img2img" ? "Instructions de modification" : "Description de l'image"}
          </label>
          <textarea
            className="locaryn-input img-gen-textarea"
            rows={3}
            placeholder={
              mode === "img2img"
                ? "Ex: Transforme ce chat en guerrier cybernétique avec un casque néon..."
                : "Ex: Une peinture à l'huile d'un chat cybernétique sous la pluie néon, 4K, détaillé..."
            }
            value={prompt}
            disabled={isInstalling || isGenerating}
            onChange={(e) => setPrompt(e.target.value)}
          />
        </div>

        {/* ── Advanced options ── */}
        <button
          type="button"
          className={`img-gen-advanced-toggle${showAdvanced ? " img-gen-advanced-open" : ""}`}
          onClick={() => setShowAdvanced((v) => !v)}
          disabled={isGenerating}
        >
          {showAdvanced ? "▼" : "▶"} Options avancées
          <span className="img-gen-advanced-summary">
            {steps} steps · CFG {cfg_scale} · {width}×{height}
          </span>
        </button>

        {showAdvanced && (
          <div className="img-gen-advanced-panel">
            <div className="img-gen-adv-row">
              <div className="img-gen-adv-item">
                <label className="img-gen-label">Prompt négatif</label>
                <input
                  type="text"
                  className="locaryn-input"
                  placeholder="blurry, ugly, watermark..."
                  value={negPrompt}
                  onChange={(e) => setNegPrompt(e.target.value)}
                  disabled={isGenerating}
                />
              </div>
            </div>
            <div className="img-gen-adv-row">
              <div className="img-gen-adv-item">
                <label className="img-gen-label">Steps <span className="img-gen-val">{steps}</span></label>
                <input type="range" className="perf-slider" min={5} max={50} value={steps} onChange={(e) => setSteps(Number(e.target.value))} disabled={isGenerating} />
              </div>
              <div className="img-gen-adv-item">
                <label className="img-gen-label">CFG Scale <span className="img-gen-val">{cfg_scale}</span></label>
                <input type="range" className="perf-slider" min={1} max={20} value={cfg_scale} onChange={(e) => setCfgScale(Number(e.target.value))} disabled={isGenerating} />
              </div>
            </div>
            <div className="img-gen-adv-row">
              {([256, 512, 768, 1024] as const).map((s) => (
                <button
                  key={s}
                  type="button"
                  className={`perf-ctx-btn${width === s && height === s ? " perf-ctx-btn-active" : ""}`}
                  onClick={() => { setWidth(s); setHeight(s); }}
                  disabled={isGenerating}
                >
                  {s}×{s}
                </button>
              ))}
            </div>

            {/* RAM/VRAM dispatch — run big diffusion models (Z-Image) on any PC */}
            <div className="img-gen-adv-item" style={{ marginTop: 12 }}>
              <label className="img-gen-label">Mémoire (dispatch RAM/VRAM)</label>
              <div className="img-gen-adv-row" style={{ marginTop: 4 }}>
                {([
                  { id: "gpu", label: "GPU", hint: "Tout en VRAM — le plus rapide, exige assez de VRAM" },
                  { id: "auto", label: "Auto", hint: "Place diffusion/encodeur/VAE selon la VRAM libre (recommandé)" },
                  { id: "lowvram", label: "Low VRAM", hint: "Poids en RAM, streamés vers la VRAM — tourne sur petit GPU" },
                ] as const).map((m) => (
                  <button
                    key={m.id}
                    type="button"
                    className={`perf-ctx-btn${vramMode === m.id ? " perf-ctx-btn-active" : ""}`}
                    onClick={() => setVramMode(m.id)}
                    disabled={isGenerating}
                    title={m.hint}
                  >
                    {m.label}
                  </button>
                ))}
              </div>
              <span className="img-gen-hint">
                {vramMode === "gpu" && "Tout en VRAM — le plus rapide, mais peut saturer un petit GPU."}
                {vramMode === "auto" && "sd.cpp répartit automatiquement selon la VRAM libre (recommandé)."}
                {vramMode === "lowvram" && "Poids gardés en RAM et streamés vers la VRAM — fait tourner Z-Image sur un petit GPU."}
              </span>
            </div>

            {/* Uncensored mode — uses an abliterated text encoder, gated by consent */}
            <div className="img-gen-adv-item" style={{ marginTop: 12 }}>
              <label className="locaryn-checkbox-row" style={{ cursor: "pointer" }}>
                <input
                  type="checkbox"
                  checked={uncensored}
                  disabled={isGenerating}
                  onChange={(e) => {
                    if (e.target.checked) setGateOpen(true);
                    else setUncensored(false);
                  }}
                />
                <span>🔓 Mode sans limite (encodeur abliteré)</span>
              </label>
              <span className="img-gen-hint">
                {uncensored
                  ? "Actif — utilise l'encodeur de texte abliteré s'il est installé. Vous êtes responsable du contenu généré."
                  : "Retire les garde-fous du modèle. Nécessite un encodeur abliteré installé + acceptation de responsabilité."}
              </span>
            </div>
          </div>
        )}

        <ResponsibilityGate
          open={gateOpen}
          what="la génération d'images sans garde-fous"
          onAccept={() => {
            // Full heretic setup: diffusion checkpoint + abliterated encoder.
            if (hereticBase) setSelectedModel(hereticBase);
            setUncensored(true);
            setGateOpen(false);
          }}
          onCancel={() => { setUncensored(false); setGateOpen(false); }}
        />

        <ResponsibilityGate
          open={nsfwGateOpen}
          what="la génération avec un modèle classé NSFW / sans garde-fous"
          onAccept={() => {
            setNsfwAccepted(true);
            setNsfwGateOpen(false);
            if (nsfwInstallPending) {
              setNsfwInstallPending(false);
              handleInstall(true);
            } else {
              handleGenerate();
            }
          }}
          onCancel={() => { setNsfwInstallPending(false); setNsfwGateOpen(false); }}
        />

        {/* ── Generation progress / result area ── */}
        {/* Driven by the active background job so reopening shows live progress. */}
        <div className="img-gen-output-area">
          {jobRunning ? (
            <div className="img-gen-generating-wrap">
              <GeneratingPlaceholder mode={mode} />
              <div className="img-gen-progress-bar-wrap">
                <div className="img-gen-progress-bar">
                  <div className="img-gen-progress-fill" style={{ width: `${activeTask?.progress ?? 0}%` }} />
                </div>
                <span className="img-gen-elapsed">{activeTask?.detail ?? "…"}</span>
              </div>
            </div>
          ) : activeTask?.status === "done" && activeTask.resultImageUrl ? (
            <div className="img-gen-result">
              <div className="img-gen-result-label">✓ Image générée</div>
              <img
                src={activeTask.resultImageUrl}
                alt="Résultat IA"
                className="img-gen-result-img"
                onError={(e) => { (e.target as HTMLElement).style.display = "none"; }}
              />
            </div>
          ) : activeTask?.status === "error" ? (
            <div className="img-gen-result-label" style={{ color: "var(--danger)" }}>
              Échec : {activeTask.error}
            </div>
          ) : null}
        </div>

        {/* ── Error ── */}
        {error && (
          <div className="img-gen-error">
            <span>⚠️</span>
            <span>{error}</span>
          </div>
        )}

        {/* ── Actions ── */}
        <div className="img-gen-actions">
          <span
            className="img-gen-estimate"
            style={{
              fontSize: "12px",
              color: "var(--text-faint)",
              display: "flex",
              alignItems: "center",
              gap: "4px",
            }}
            title="Estimation basée sur les durées de tes dernières générations avec ce modèle et ces paramètres"
          >
            ⏱️ Durée estimée : {formatEstimatedDuration(estimatedDuration)}
          </span>
          {!inline && (
            <button type="button" className="locaryn-btn-ghost" onClick={onClose} disabled={isInstalling}>
              Fermer
            </button>
          )}
          <button
            type="button"
            className={`img-gen-generate-btn${jobRunning ? " img-gen-generate-btn-busy" : ""}`}
            disabled={!prompt.trim() || jobRunning || isInstalling || !isSelectedInstalled || (mode === "img2img" && !sourceImagePath)}
            onClick={handleGenerate}
          >
            {jobRunning ? (
              <>
                <span className="img-gen-spinner-inline">◌</span>
                {mode === "img2img" ? "Transformation…" : "Génération…"}
              </>
            ) : (
              <>{mode === "img2img" ? "🖼️ Transformer" : "🎨 Générer"}</>
            )}
          </button>
        </div>

    </div>
  );

  if (inline) return panelContent;
  return <div className="img-gen-overlay">{panelContent}</div>;
}
