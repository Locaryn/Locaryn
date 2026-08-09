import { useEffect, useMemo, useRef, useState } from "react";
import { type AudioJobResult, startAudioGeneration, toAudioUrl } from "../lib/audioJobs";
import {
  TTS_SAMPLING_DEFAULTS,
  VOICE_SETTINGS_DEFAULTS,
  type VoicePreset,
  core,
} from "../lib/core";
import { loadMediaObjectUrl } from "../lib/media";
import { dedupeModelsByDirectory, normalizeModelPath } from "../lib/modelList";
import { TTS_MODELS, getModelCapabilities } from "../lib/modelRegistry";
import { taskCenter } from "../lib/taskCenter";
import { VoiceCloneTab, VoiceCustomTab, VoiceDesignTab } from "./voice";

// ── Types ──────────────────────────────────────────────────────────────────

type Props = {
  installedModels: string[];
  onClose: () => void;
  inline?: boolean;
};

type StudioTab = "design" | "clone" | "tts";

// ── Language options ─────────────────────────────────────────────────────────

const LANGUAGE_OPTIONS = [
  { code: "auto", label: "Auto" },
  { code: "fr", label: "Français" },
  { code: "en", label: "English" },
  { code: "es", label: "Español" },
  { code: "de", label: "Deutsch" },
  { code: "it", label: "Italiano" },
  { code: "pt", label: "Português" },
  { code: "zh", label: "中文" },
  { code: "ja", label: "日本語" },
  { code: "ko", label: "한국어" },
  { code: "ru", label: "Русский" },
];

const TAB_LABELS: Record<StudioTab, { label: string; icon: string }> = {
  design: { label: "Voice Design", icon: "✨" },
  clone: { label: "Voice Clone (Base)", icon: "🧬" },
  tts: { label: "TTS (CustomVoice)", icon: "🗣️" },
};

// ── Model name formatter ────────────────────────────────────────────────────

interface TtsModelInfo {
  id: string;
  engine: string;
  name: string;
  lang: string;
  quality: string;
}

function parseModelName(raw: string): TtsModelInfo {
  const normalized = normalizeModelPath(raw);
  const lower = normalized.toLowerCase();
  const repoName = normalized.split("/").pop() ?? normalized;

  if (lower.includes("pocket-tts") || lower.includes("pocket_tts") || lower.includes("pockettts")) {
    return {
      id: raw,
      engine: "Pocket TTS (Kyutai)",
      name: "Pocket TTS",
      lang: "English",
      quality: "Clonage",
    };
  }

  for (const family of TTS_MODELS) {
    const hitsFamily =
      lower.includes(family.id.toLowerCase()) || lower.includes(family.name.toLowerCase());
    const hitsVariant = family.variants.some((v) => lower.includes(v.tag.toLowerCase()));
    if (hitsFamily || hitsVariant) {
      const caps = getModelCapabilities(raw);
      return {
        id: raw,
        engine: family.brand,
        name: family.name,
        lang: caps.languages.includes("all") ? "Multilingue" : caps.languages.join(", ") || "-",
        quality: caps.cloning ? "Clonage" : caps.voiceDesign ? "Voice Design" : "TTS",
      };
    }
  }

  if (lower.includes("piper")) {
    const stripped = repoName.replace(/\.onnx$/i, "");
    const m = stripped.match(/^([a-z]+)_([A-Z]+)-(.+)-(low|medium|high)$/i);
    if (m) {
      const [, lang, region, voice, quality] = m;
      return {
        id: raw,
        engine: "Piper",
        name: `${voice} (${region})`,
        lang: `${lang}_${region}`,
        quality,
      };
    }
    return { id: raw, engine: "Piper", name: stripped, lang: "-", quality: "-" };
  }

  if (lower.includes("coqui") || lower.includes("xtts")) {
    return {
      id: raw,
      engine: "Coqui XTTS",
      name: raw.includes("v2") ? "XTTS v2" : "XTTS",
      lang: "Multilingue",
      quality: "Clonage",
    };
  }

  if (lower.includes("qwen3")) {
    let variant = "Base";
    if (lower.includes("customvoice")) variant = "CustomVoice";
    else if (lower.includes("voicedesign")) variant = "VoiceDesign";
    return {
      id: raw,
      engine: "Qwen3-TTS",
      name: `Qwen3-TTS (${variant})`,
      lang: "Multilingue",
      quality:
        variant === "CustomVoice" ? "Clonage" : variant === "VoiceDesign" ? "Voice Design" : "TTS",
    };
  }

  if (lower.includes("kokoro"))
    return { id: raw, engine: "Kokoro", name: repoName, lang: "Multilingue", quality: "TTS" };
  if (lower.includes("parler"))
    return {
      id: raw,
      engine: "Parler-TTS",
      name: repoName,
      lang: "Multilingue",
      quality: "Voice Design",
    };
  if (lower.includes("melotts"))
    return { id: raw, engine: "MeloTTS", name: repoName, lang: "Multilingue", quality: "TTS" };
  if (lower.includes("f5-tts") || lower.includes("f5tts") || lower.includes("f5_tts"))
    return { id: raw, engine: "F5-TTS", name: repoName, lang: "Multilingue", quality: "Clonage" };
  if (lower.includes("chatterbox"))
    return { id: raw, engine: "Chatterbox", name: repoName, lang: "English", quality: "TTS" };
  if (lower.includes("moss-tts") || lower.includes("mosstts"))
    return { id: raw, engine: "MOSS-TTS", name: repoName, lang: "Multilingue", quality: "TTS" };
  if (lower.includes("higgs-tts") || lower.includes("higgstts"))
    return { id: raw, engine: "Higgs-TTS", name: repoName, lang: "Multilingue", quality: "TTS" };
  if (lower.includes("vibevoice"))
    return { id: raw, engine: "VibeVoice", name: repoName, lang: "Multilingue", quality: "TTS" };
  if (lower.includes("voxcpm2"))
    return { id: raw, engine: "VoxCPM2", name: repoName, lang: "Multilingue", quality: "TTS" };
  if (lower.includes("omnivoice"))
    return { id: raw, engine: "OmniVoice", name: repoName, lang: "Multilingue", quality: "TTS" };

  const clean = repoName.replace(/\.(onnx|pt|bin|pth|safetensors)$/i, "");
  return { id: raw, engine: "Local", name: clean, lang: "-", quality: "-" };
}

function isTtsModel(m: string): boolean {
  const lower = m.toLowerCase();
  const ttsKeywords =
    /piper|xtts|coqui|melotts|kokoro|parler|chatterbox|voxcpm2|omnivoice|f5[-_.]?tts|qwen3[-_.]?tts|moss[-_.]?tts|higgs[-_.]?tts|vibevoice|pocket[-_.]?tts/;
  if (ttsKeywords.test(lower)) {
    if (/tokenizer|config\.json|vocab\.json|merges\.txt|preprocessor_config/i.test(lower))
      return false;
    return true;
  }
  return lower.endsWith(".onnx");
}

function getTtsModels(models: string[]): string[] {
  return dedupeModelsByDirectory(models.filter(isTtsModel));
}

function firstTtsModel(models: string[]): string | undefined {
  return getTtsModels(models)[0];
}

function detectLanguageFromText(textValue: string): string | null {
  const lower = textValue.trim().toLowerCase();
  if (!lower) return null;
  const markers: Record<string, string[]> = {
    fr: [
      " le ",
      " la ",
      " et ",
      " un ",
      " une ",
      " est ",
      " je ",
      " de la ",
      " du ",
      " des ",
      " que ",
      " qui ",
      " dans ",
      " pour ",
      " les ",
    ],
    en: [
      " the ",
      " is ",
      " a ",
      " and ",
      " to ",
      " of ",
      " in ",
      " you ",
      " that ",
      " it ",
      " for ",
      " on ",
    ],
    es: [" el ", " la ", " y ", " de ", " que ", " en ", " un ", " una ", " los ", " las "],
    de: [" der ", " die ", " und ", " das ", " zu ", " ein ", " eine ", " ist ", " von ", " mit "],
    it: [" il ", " la ", " e ", " di ", " che ", " un ", " una ", " in ", " per "],
    pt: [" o ", " a ", " e ", " de ", " do ", " da ", " um ", " uma ", " em ", " para ", " que "],
  };
  let best: string | null = null;
  let bestScore = 0;
  for (const [lang, words] of Object.entries(markers)) {
    const score = words.reduce((acc, w) => acc + (lower.includes(w) ? 1 : 0), 0);
    if (score > bestScore) {
      bestScore = score;
      best = lang;
    }
  }
  return bestScore >= 2 ? best : null;
}

// ── Main component ─────────────────────────────────────────────────────────

export function AudioGenPanel({ installedModels, onClose, inline }: Props) {
  // ── Tab state
  const [activeTab, setActiveTab] = useState<StudioTab>("design");

  // ── Core state
  const [text, setText] = useState("");
  const [selectedModel, setSelectedModel] = useState<string>(
    () => firstTtsModel(installedModels) ?? "piper-voices/en_US-amy-medium.onnx",
  );
  const [modelSearch, setModelSearch] = useState("");
  const [synthesisLang, setSynthesisLang] = useState<string>("auto");

  // ── Clone-specific state
  const [voiceFile, setVoiceFile] = useState<string | null>(null);
  const [voiceName, setVoiceName] = useState<string>("");
  const [voiceFileUrl, setVoiceFileUrl] = useState<string | null>(null);
  const [referenceText, setReferenceText] = useState("");
  // Engine default. The 0.7 this panel used to imply read as monotone; higher
  // values give the intonation more room to move.
  const [temperature, setTemperature] = useState(TTS_SAMPLING_DEFAULTS.temperature);
  // In-context cloning: carries the reference speaker's rhythm, not just timbre.
  const [expressive, setExpressive] = useState(true);
  // Silence stretch, applied after rendering so it behaves the same on every
  // engine rather than only those exposing a pause control.
  const [pauseScale, setPauseScale] = useState(VOICE_SETTINGS_DEFAULTS.pauseScale);
  const [instruct, setInstruct] = useState("");
  const [isRecording, setIsRecording] = useState(false);
  const [recordTime, setRecordTime] = useState(0);

  // ── Generation parameters (clone settings)
  const [speed, setSpeed] = useState(1.0);
  const [pitch, setPitch] = useState(1.0);
  const [energy, setEnergy] = useState(0.7);
  const [clarity, setClarity] = useState(0.8);

  // ── Design state
  const [designPrompt, setDesignPrompt] = useState("");

  // ── TTS CustomVoice state
  const [speaker, setSpeaker] = useState("default");
  const [styleInstruction, setStyleInstruction] = useState("");

  // ── Generation results
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [generatedResult, setGeneratedResult] = useState<AudioJobResult | null>(null);
  const [taskProgress, setTaskProgress] = useState<{
    progress: number;
    detail?: string;
    status?: string;
  } | null>(null);
  const [statusMessage, setStatusMessage] = useState("");

  // ── Refs
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const recordedChunksRef = useRef<Blob[]>([]);
  const recordTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const audioUrlRef = useRef<string | null>(null);
  const progressPollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    return () => {
      if (audioUrlRef.current) URL.revokeObjectURL(audioUrlRef.current);
      if (progressPollRef.current) clearInterval(progressPollRef.current);
    };
  }, []);

  // ── TTS model filtering ────────────────────────────────────────────────

  const ttsModels = useMemo(() => getTtsModels(installedModels), [installedModels]);

  const searchedModels = useMemo(() => {
    if (!modelSearch.trim()) return ttsModels;
    const term = modelSearch.toLowerCase();
    return ttsModels.filter((m) => {
      if (m === selectedModel) return true;
      const info = parseModelName(m);
      return (
        m.toLowerCase().includes(term) ||
        info.engine.toLowerCase().includes(term) ||
        info.name.toLowerCase().includes(term)
      );
    });
  }, [ttsModels, modelSearch, selectedModel]);

  const groupedSearchedModels = useMemo(() => {
    const groups: Record<string, string[]> = {};
    for (const m of searchedModels) {
      const info = parseModelName(m);
      if (!groups[info.engine]) groups[info.engine] = [];
      groups[info.engine].push(m);
    }
    return groups;
  }, [searchedModels]);

  const selectedInfo = parseModelName(selectedModel);
  const selectedCaps = getModelCapabilities(selectedModel);

  useEffect(() => {
    if (ttsModels.length === 0) return;
    if (!ttsModels.includes(selectedModel)) {
      setSelectedModel(ttsModels[0]);
    }
  }, [ttsModels, selectedModel]);

  const caps = useMemo(() => getModelCapabilities(selectedModel), [selectedModel]);

  // Auto-select the best tab based on model capabilities
  // biome-ignore lint/correctness/useExhaustiveDependencies: `activeTab` est volontairement absent — les onglets restent tous cliquables, donc réagir à chaque clic ramènerait aussitôt l'utilisateur sur un autre onglet dès que le modèle courant n'annonce pas la capacité correspondante. `caps` suit déjà `selectedModel` (useMemo), qui devient donc redondant.
  useEffect(() => {
    if (caps.voiceDesign && activeTab === "design") return;
    if (caps.cloning && activeTab === "clone") return;
    if (activeTab === "tts") return;
    // Pick best available tab
    if (caps.voiceDesign) setActiveTab("design");
    else if (caps.cloning) setActiveTab("clone");
    else setActiveTab("tts");
  }, [caps]);

  // ── Voice file URL conversion for clone preview ──────────────────────
  useEffect(() => {
    if (!voiceFile) {
      setVoiceFileUrl(null);
      return;
    }
    if (voiceFile.startsWith("data:") || voiceFile.startsWith("blob:")) {
      setVoiceFileUrl(voiceFile);
      return;
    }
    // Load into a blob so the element gets an explicit MIME type and can
    // report a duration. Revoked on change to avoid leaking object URLs.
    let revoked: string | null = null;
    let cancelled = false;
    loadMediaObjectUrl(voiceFile)
      .then((url) => {
        if (cancelled) {
          URL.revokeObjectURL(url);
          return;
        }
        revoked = url;
        setVoiceFileUrl(url);
      })
      .catch((e) => {
        if (cancelled) return;
        // No silent fallback: a URL the webview cannot load renders as
        // "0:00 / 0:00", which looks like a broken file rather than a bug.
        setVoiceFileUrl(null);
        setError(
          `Impossible de lire ${voiceFile.split(/[/\\]/).pop()} : ${
            e instanceof Error ? e.message : String(e)
          }`,
        );
      });
    return () => {
      cancelled = true;
      if (revoked) URL.revokeObjectURL(revoked);
    };
  }, [voiceFile]);

  /** Restore a saved voice wholesale: recording, transcript and settings.
   *  The recording lives inside the preset, so nothing has to be re-uploaded. */
  function applyPreset(p: VoicePreset) {
    setVoiceFile(p.referenceAudio || null);
    setVoiceName(p.name);
    setReferenceText(p.referenceText ?? "");
    if (p.language) setSynthesisLang(p.language);
    const s = p.settings;
    setSpeed(s.speed);
    setPitch(s.pitch);
    setEnergy(s.energy);
    setClarity(s.clarity);
    setPauseScale(s.pauseScale);
    setTemperature(s.temperature);
    setExpressive(s.expressive);
    setInstruct(s.instruct ?? "");
  }

  // ── Voice reference helpers ──────────────────────────────────────────────

  async function handlePickVoice() {
    try {
      const path = await core.pickVoiceReference();
      if (!path) return;
      setVoiceFile(path);
      setVoiceName(path.split(/[/\\]/).pop() ?? path);
    } catch (e) {
      setError(
        typeof e === "string"
          ? e
          : (e as Error)?.message || "Impossible de choisir le fichier vocal.",
      );
    }
  }

  function updateRecording(blob: Blob) {
    if (audioUrlRef.current) URL.revokeObjectURL(audioUrlRef.current);
    const url = URL.createObjectURL(blob);
    audioUrlRef.current = url;
    setVoiceFile(url);
    setVoiceName(`enregistrement_${new Date().toISOString().slice(0, 19).replace(/:/g, "-")}.wav`);
  }

  async function startRecordingFn() {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const mediaRecorder = new MediaRecorder(stream);
      mediaRecorderRef.current = mediaRecorder;
      recordedChunksRef.current = [];

      mediaRecorder.ondataavailable = (event) => {
        if (event.data.size > 0) recordedChunksRef.current.push(event.data);
      };

      mediaRecorder.onstop = () => {
        const blob = new Blob(recordedChunksRef.current, { type: "audio/wav" });
        updateRecording(blob);
      };

      mediaRecorder.start();
      setIsRecording(true);
      setRecordTime(0);
      recordTimerRef.current = setInterval(() => setRecordTime((t) => t + 1), 1000);
    } catch (err) {
      setError("Impossible d'accéder au micro. Vérifiez les permissions.");
    }
  }

  function stopRecordingFn() {
    mediaRecorderRef.current?.stop();
    setIsRecording(false);
    if (recordTimerRef.current) clearInterval(recordTimerRef.current);
  }

  function clearVoice() {
    setVoiceFile(null);
    setVoiceName("");
    setVoiceFileUrl(null);
    if (audioUrlRef.current) {
      URL.revokeObjectURL(audioUrlRef.current);
      audioUrlRef.current = null;
    }
  }

  // ── Generation ─────────────────────────────────────────────────────────

  function pollTask(taskId: string) {
    setTaskProgress({ progress: 0, detail: "En attente…", status: "running" });
    const interval = setInterval(() => {
      const all = taskCenter.snapshot();
      const task = all.find((t) => t.id === taskId);
      if (task) {
        setTaskProgress({ progress: task.progress ?? 0, detail: task.detail, status: task.status });
        setStatusMessage(task.detail ?? "");
        if (task.status === "done" && task.resultAudioUrl) {
          setGeneratedResult({
            url: task.resultAudioUrl,
            path: task.resultAudioUrl,
            simulated: task.detail?.includes("simulé") ?? false,
          });
          setStatusMessage("Génération terminée ✓");
          clearInterval(interval);
          setIsGenerating(false);
        }
        if (task.status === "error") {
          setStatusMessage(`Erreur : ${task.detail ?? "inconnue"}`);
          clearInterval(interval);
          setIsGenerating(false);
        }
      }
    }, 300);
    return interval;
  }

  async function handleGenerate() {
    if (!text.trim()) {
      setError("Veuillez saisir un texte à synthétiser.");
      return;
    }
    if (activeTab === "clone" && !voiceFile) {
      setError("Veuillez fournir un échantillon vocal à cloner.");
      return;
    }
    setError(null);
    setIsGenerating(true);
    setGeneratedResult(null);
    setTaskProgress(null);
    setStatusMessage("Démarrage de la génération…");

    try {
      const appInfo = await core.appInfo().catch(() => ({ data_dir: "/tmp" }));
      const outputDir = `${appInfo.data_dir}/generated_audio`;
      const detected = detectLanguageFromText(text) ?? "auto";
      const langToSend = synthesisLang === "auto" ? detected : synthesisLang;

      const params: Record<string, unknown> = {
        model: selectedModel,
        text: text.trim(),
        outputDir,
        language: langToSend,
      };

      if (activeTab === "clone") {
        params.voiceReference = voiceFile ?? undefined;
        params.speed = speed;
        params.pitch = pitch;
        params.energy = energy;
        params.clarity = clarity;
        if (instruct.trim()) params.voiceDescription = instruct.trim();
        // The reference transcript was collected but never sent, so in-context
        // cloning could never engage and every clone came out flat.
        params.sampling = {
          ...TTS_SAMPLING_DEFAULTS,
          temperature,
          expressive,
          pauseScale,
          // Qwen3-TTS has no pitch/energy/clarity of its own; these are applied
          // to the rendered audio, so they work on every engine.
          pitch,
          energy,
          clarity,
          referenceText: referenceText.trim(),
        };
      } else if (activeTab === "design") {
        params.designPrompt = designPrompt.trim() || undefined;
      } else if (activeTab === "tts") {
        params.speaker = speaker;
        if (styleInstruction.trim()) params.voiceDescription = styleInstruction.trim();
      }

      const taskId = startAudioGeneration(
        params as unknown as Parameters<typeof startAudioGeneration>[0],
      );
      if (progressPollRef.current) clearInterval(progressPollRef.current);
      progressPollRef.current = pollTask(taskId);
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error)?.message || "Échec de la synthèse vocale.");
      setIsGenerating(false);
    }
  }

  // ── Derived UI state ─────────────────────────────────────────────────────

  const jobRunning = taskProgress?.status === "running" || isGenerating;
  const hasModels = ttsModels.length > 0;

  // ── Generate button label per tab ──────────────────────────────────────

  function generateButtonLabel(): string {
    if (jobRunning) return "Génération…";
    switch (activeTab) {
      case "design":
        return "✨ Generate with Custom Voice";
      case "clone":
        return "🧬 Clone & Generate";
      case "tts":
        return "🗣️ Generate Speech";
    }
  }

  // ── Render ───────────────────────────────────────────────────────────

  return (
    <div
      className={inline ? "" : "locaryn-card"}
      style={{ padding: inline ? 0 : 24, maxWidth: 1100, margin: "0 auto" }}
    >
      {/* ── Header ── */}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 16,
        }}
      >
        <div>
          <h3 style={{ margin: 0 }}>Synthèse vocale IA</h3>
          <p className="locaryn-field-hint" style={{ margin: "4px 0 0" }}>
            Choisissez un modèle, un mode de génération, puis entrez votre texte.
          </p>
        </div>
        {!inline && (
          <button type="button" className="locaryn-icon-btn" onClick={onClose} aria-label="Fermer">
            ✕
          </button>
        )}
      </div>

      {/* ── Model picker ── */}
      {searchedModels.length === 0 ? (
        <div
          style={{
            padding: 24,
            borderRadius: 10,
            border: "1px dashed var(--border)",
            textAlign: "center",
            color: "var(--text-faint)",
            fontSize: 13,
            marginBottom: 16,
          }}
        >
          {ttsModels.length === 0
            ? "Aucun modèle TTS installé. Allez dans le Marketplace pour en installer."
            : "Aucun modèle ne correspond à la recherche."}
        </div>
      ) : (
        <div style={{ marginBottom: 16 }}>
          <input
            type="text"
            className="locaryn-input"
            value={modelSearch}
            onChange={(e) => setModelSearch(e.target.value)}
            placeholder="Rechercher un modèle…"
            disabled={jobRunning}
            style={{ width: "100%", marginBottom: 8, fontSize: 13 }}
          />
          <select
            className="locaryn-input"
            value={selectedModel}
            onChange={(e) => setSelectedModel(e.target.value)}
            disabled={jobRunning}
            style={{ width: "100%", marginBottom: 8, fontSize: 13 }}
          >
            {Object.entries(groupedSearchedModels).map(([engine, models]) => (
              <optgroup key={engine} label={engine}>
                {models.map((m) => {
                  const info = parseModelName(m);
                  return (
                    <option key={m} value={m}>
                      {info.name} {info.lang !== "-" ? `(${info.lang})` : ""}
                    </option>
                  );
                })}
              </optgroup>
            ))}
          </select>
          {/* Selected model info badges */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              flexWrap: "wrap",
            }}
          >
            <span style={{ fontSize: 12, fontWeight: 600, color: "var(--text)" }}>
              {selectedInfo.engine} — {selectedInfo.name}
            </span>
            {selectedInfo.lang !== "-" && (
              <span
                style={{
                  fontSize: 10,
                  padding: "2px 6px",
                  borderRadius: 4,
                  background: "rgba(255,255,255,0.05)",
                  color: "var(--text-faint)",
                  border: "1px solid var(--border)",
                }}
              >
                {selectedInfo.lang}
              </span>
            )}
            {selectedCaps.cloning && (
              <span
                style={{
                  fontSize: 10,
                  padding: "2px 6px",
                  borderRadius: 4,
                  background: "rgba(150, 100, 255, 0.15)",
                  color: "var(--accent)",
                  border: "1px solid var(--border)",
                }}
              >
                Clonage
              </span>
            )}
            {selectedCaps.voiceDesign && (
              <span
                style={{
                  fontSize: 10,
                  padding: "2px 6px",
                  borderRadius: 4,
                  background: "rgba(100, 200, 150, 0.12)",
                  color: "var(--text-faint)",
                  border: "1px solid var(--border)",
                }}
              >
                Voice Design
              </span>
            )}
          </div>
        </div>
      )}

      {/* ── Tabs ── */}
      <div
        role="tablist"
        style={{
          display: "flex",
          borderBottom: "2px solid var(--border)",
          marginBottom: 0,
          gap: 0,
        }}
      >
        {(Object.keys(TAB_LABELS) as StudioTab[]).map((tabId) => {
          const tab = TAB_LABELS[tabId];
          const active = activeTab === tabId;
          return (
            <button
              key={tabId}
              type="button"
              role="tab"
              aria-selected={active}
              disabled={jobRunning}
              onClick={() => setActiveTab(tabId)}
              style={{
                padding: "10px 20px",
                border: "none",
                borderBottom: active ? "2px solid var(--accent)" : "2px solid transparent",
                background: "transparent",
                color: active ? "var(--accent)" : "var(--text-faint)",
                fontSize: 13,
                fontWeight: active ? 600 : 400,
                cursor: jobRunning ? "default" : "pointer",
                transition: "color 0.15s, border-color 0.15s",
                marginBottom: -2,
              }}
            >
              {tab.label}
            </button>
          );
        })}
      </div>

      {/* ── Two-column layout ── */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr 380px",
          gap: 24,
          marginTop: 20,
          minHeight: 400,
        }}
      >
        {/* ── LEFT COLUMN: Controls ── */}
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          {/* Text to Synthesize */}
          <div>
            <label
              htmlFor="audio-gen-text"
              style={{
                display: "inline-block",
                fontSize: 12,
                fontWeight: 700,
                color: "#fff",
                background: "var(--accent)",
                padding: "2px 8px",
                borderRadius: 3,
                marginBottom: 8,
              }}
            >
              Text to Synthesize
            </label>
            <textarea
              id="audio-gen-text"
              className="locaryn-input"
              rows={4}
              placeholder="Saisissez le texte que vous souhaitez faire parler..."
              value={text}
              onChange={(e) => setText(e.target.value)}
              disabled={jobRunning}
              style={{ width: "100%", resize: "vertical" }}
            />
          </div>

          {/* Language selector — always visible */}
          <div style={{ display: "flex", gap: 12, flexWrap: "wrap", alignItems: "flex-end" }}>
            <div style={{ flex: 1, minWidth: 160 }}>
              <label
                htmlFor="audio-gen-language"
                style={{
                  display: "inline-block",
                  fontSize: 12,
                  fontWeight: 700,
                  color: "#fff",
                  background: "var(--accent)",
                  padding: "2px 8px",
                  borderRadius: 3,
                  marginBottom: 8,
                }}
              >
                Language
              </label>
              <select
                id="audio-gen-language"
                className="locaryn-input"
                value={synthesisLang}
                onChange={(e) => setSynthesisLang(e.target.value)}
                disabled={jobRunning}
                style={{ width: "100%", fontSize: 13 }}
              >
                {LANGUAGE_OPTIONS.map((opt) => (
                  <option key={opt.code} value={opt.code}>
                    {opt.label}
                  </option>
                ))}
              </select>
              {synthesisLang === "auto" && (
                <p style={{ margin: "4px 0 0", fontSize: 11, color: "var(--text-faint)" }}>
                  Keep as Auto to auto-detect the language.
                </p>
              )}
            </div>
          </div>

          {/* Tab-specific content */}
          {activeTab === "design" && (
            <VoiceDesignTab
              designPrompt={designPrompt}
              jobRunning={jobRunning}
              onPromptChange={setDesignPrompt}
            />
          )}

          {activeTab === "clone" && (
            <VoiceCloneTab
              voiceFile={voiceFile}
              voiceFileUrl={voiceFileUrl}
              voiceName={voiceName}
              referenceText={referenceText}
              instruct={instruct}
              isRecording={isRecording}
              recordTime={recordTime}
              speed={speed}
              temperature={temperature}
              expressive={expressive}
              onTemperatureChange={setTemperature}
              onExpressiveChange={setExpressive}
              model={selectedModel}
              language={synthesisLang}
              pauseScale={pauseScale}
              onPauseScaleChange={setPauseScale}
              onApplyPreset={applyPreset}
              pitch={pitch}
              energy={energy}
              clarity={clarity}
              jobRunning={jobRunning}
              onPickVoice={handlePickVoice}
              onStartRecording={startRecordingFn}
              onStopRecording={stopRecordingFn}
              onClearVoice={clearVoice}
              onReferenceTextChange={setReferenceText}
              onInstructChange={setInstruct}
              onSpeedChange={setSpeed}
              onPitchChange={setPitch}
              onEnergyChange={setEnergy}
              onClarityChange={setClarity}
            />
          )}

          {activeTab === "tts" && (
            <VoiceCustomTab
              speaker={speaker}
              styleInstruction={styleInstruction}
              jobRunning={jobRunning}
              onSpeakerChange={setSpeaker}
              onStyleInstructionChange={setStyleInstruction}
            />
          )}

          {/* Error display */}
          {error && (
            <div className="img-gen-error" style={{ marginBottom: 0 }}>
              <span>⚠️</span>
              <span>{error}</span>
            </div>
          )}

          {/* Generate button */}
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={handleGenerate}
            disabled={!text.trim() || jobRunning || isGenerating || !hasModels}
            style={{
              width: "100%",
              padding: "14px 0",
              fontSize: 15,
              fontWeight: 700,
              borderRadius: 8,
              marginTop: "auto",
            }}
          >
            {generateButtonLabel()}
          </button>
        </div>

        {/* ── RIGHT COLUMN: Output ── */}
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          {/* Generated Audio */}
          <div
            style={{
              padding: 16,
              borderRadius: 8,
              border: "1px solid var(--border)",
              background: "rgba(100, 150, 255, 0.04)",
            }}
          >
            <div
              style={{
                display: "inline-block",
                fontSize: 12,
                fontWeight: 700,
                color: "#fff",
                background: "var(--accent)",
                padding: "2px 8px",
                borderRadius: 3,
                marginBottom: 12,
              }}
            >
              🎵 Generated Audio
            </div>
            {generatedResult ? (
              // biome-ignore lint/a11y/useMediaCaption: l'audio vient d'être synthétisé localement, il n'existe aucune piste de sous-titres à lui associer
              <audio
                key={generatedResult.url}
                src={generatedResult.url}
                controls
                preload="auto"
                style={{ width: "100%", marginTop: 8 }}
              />
            ) : (
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  padding: "40px 0",
                  color: "var(--text-faint)",
                  fontSize: 32,
                }}
              >
                🎵
              </div>
            )}
          </div>

          {/* Status */}
          <div
            style={{
              padding: 16,
              borderRadius: 8,
              border: "1px solid var(--border)",
              background: "rgba(100, 150, 255, 0.04)",
            }}
          >
            <div
              style={{
                display: "inline-block",
                fontSize: 12,
                fontWeight: 700,
                color: "#fff",
                background: "var(--accent)",
                padding: "2px 8px",
                borderRadius: 3,
                marginBottom: 12,
              }}
            >
              Status
            </div>
            {jobRunning && taskProgress && (
              <div style={{ marginTop: 8 }}>
                <div className="img-gen-progress-bar">
                  <div
                    className="img-gen-progress-fill"
                    style={{ width: `${taskProgress.progress ?? 0}%` }}
                  />
                </div>
              </div>
            )}
            <div style={{ fontSize: 12, color: "var(--text-faint)", marginTop: 8, minHeight: 20 }}>
              {statusMessage || (jobRunning ? "Génération en cours…" : "")}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
