import { Icon, type IconName } from "@locaryn/ui-core";
import { useCallback, useEffect, useRef, useState } from "react";
import { type InferenceConfig, type InferenceProfile, type KvCacheType, core } from "../lib/core";

// ── Types ─────────────────────────────────────────────────────────────────────

interface ProfileCard {
  id: InferenceProfile;
  icon: IconName;
  label: string;
  tagline: string;
  details: string[];
  /** Mention fixe, vraie quelle que soit la machine. « Recommandé » n'en est pas
   *  une : il se calcule d'après la mémoire vidéo (voir `profilConseille`). */
  badge?: string;
}

const PROFILES: ProfileCard[] = [
  {
    id: "auto",
    icon: "speed",
    label: "Automatique",
    tagline: "Le modèle entier sur la carte, le reste en contexte",
    details: [
      "Toutes les couches sur le GPU quand elles tiennent",
      "Contexte = mémoire vidéo restante (8K au moins)",
      "Cache Q8 (÷2 VRAM, sans perte visible)",
      "Flash Attention",
    ],
  },
  {
    id: "eco",
    icon: "cloud",
    label: "Économe",
    tagline: "Processeur seul, la carte reste libre",
    details: ["0 couche sur le GPU", "Cache FP16 standard", "Contexte 4K", "Bien plus lent"],
  },
];

/** Les profils d'avant, toujours lisibles dans une configuration enregistrée. */
const ANCIENS_PROFILS: Record<string, string> = {
  balanced: "Équilibré",
  performance: "Performance",
  turbo: "Turbo",
  longctx: "Contexte long",
};

/** Nom lisible d'un profil, y compris « custom » : la pastille affichait le
 *  jeton interne (« balanced »). */
function nomDuProfil(id: string): string {
  if (id === "custom") return "Personnalisé";
  return PROFILES.find((p) => p.id === id)?.label ?? ANCIENS_PROFILS[id] ?? id;
}

/** Le profil qui convient à la machine : sans carte graphique, le processeur ;
 *  avec, l'automatique, qui s'adapte à sa mémoire. */
function profilConseille(vramGo: number): InferenceProfile {
  return vramGo <= 0 ? "eco" : "auto";
}

/** « Auto » quand le moteur choisit la fenêtre, sinon « 16K ». */
function contexteLisible(n: number): string {
  if (n === 0) return "Auto";
  return n >= 1024 ? `${Math.round(n / 1024)}K` : `${n}`;
}

const KV_OPTIONS: { value: KvCacheType; label: string; desc: string; color: string }[] = [
  { value: "f16", label: "FP16", desc: "Standard", color: "var(--text-faint)" },
  { value: "q8_0", label: "Q8", desc: "÷2 VRAM", color: "var(--accent)" },
  { value: "q4_0", label: "Q4", desc: "÷4 VRAM (max réel)", color: "var(--warn)" },
];

const CTX_PRESETS = [
  { label: "Auto", value: 0 },
  { label: "2K", value: 2048 },
  { label: "4K", value: 4096 },
  { label: "8K", value: 8192 },
  { label: "16K", value: 16384 },
  { label: "32K", value: 32768 },
  { label: "65K", value: 65536 },
  { label: "128K", value: 131072 },
];

// ── Component ─────────────────────────────────────────────────────────────────

export function PerformancePanel() {
  const [cfg, setCfg] = useState<InferenceConfig | null>(null);
  const [hw, setHw] = useState<{ vram: number; ram: number; cores: number } | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [showExpert, setShowExpert] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const savedTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // La dernière configuration connue, lue hors des mises à jour d'état : les
  // enregistrer depuis l'intérieur d'un setter les déclenchait deux fois en
  // mode strict, et sur une valeur qui n'était pas toujours la dernière.
  const cfgRef = useRef<InferenceConfig | null>(null);

  useEffect(() => {
    core
      .getInferenceConfig()
      .then((c) => {
        cfgRef.current = c;
        setCfg(c);
      })
      .catch((e: unknown) => setLoadError(String(e).replace(/^Error:\s*/, "")));
    core
      .checkHardware()
      .then((h) => setHw({ vram: h.total_vram_gb, ram: h.total_ram_gb, cores: h.cpu_cores ?? 4 }))
      .catch((e: unknown) => console.warn("matériel illisible", e));
    return () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      if (savedTimer.current) clearTimeout(savedTimer.current);
    };
  }, []);

  const autoSave = useCallback((newCfg: InferenceConfig) => {
    if (saveTimer.current) clearTimeout(saveTimer.current);
    setSaved(false);
    saveTimer.current = setTimeout(async () => {
      setSaving(true);
      try {
        await core.setInferenceConfig(newCfg);
        // Le panneau du modèle et la jauge du chat relisent la fenêtre réglée.
        window.dispatchEvent(new Event("locaryn:inference-config-changed"));
        setSaved(true);
        if (savedTimer.current) clearTimeout(savedTimer.current);
        savedTimer.current = setTimeout(() => setSaved(false), 2500);
      } catch (e) {
        setLoadError(`Enregistrement impossible : ${String(e).replace(/^Error:\s*/, "")}`);
      } finally {
        setSaving(false);
      }
    }, 600);
  }, []);

  const commit = useCallback(
    (next: InferenceConfig) => {
      cfgRef.current = next;
      setCfg(next);
      setLoadError(null);
      autoSave(next);
    },
    [autoSave],
  );

  const patch = useCallback(
    (delta: Partial<InferenceConfig>) => {
      const prev = cfgRef.current;
      if (!prev) return;
      const next = { ...prev, ...delta };
      // Tout réglage manuel donne un profil personnalisé.
      if (!("profile" in delta)) next.profile = "custom";
      commit(next);
    },
    [commit],
  );

  const applyProfile = useCallback(
    async (id: InferenceProfile) => {
      try {
        const preset = await core.getProfilePreset(id);
        // On garde le modèle « draft » déjà choisi.
        commit({ ...preset, draft_model_path: cfgRef.current?.draft_model_path ?? "" });
      } catch (e) {
        setLoadError(`Profil indisponible : ${String(e).replace(/^Error:\s*/, "")}`);
      }
    },
    [commit],
  );

  if (loadError && !cfg) return <div className="perf-loading">{loadError}</div>;
  if (!cfg) return <div className="perf-loading">Chargement…</div>;

  const conseille = hw ? profilConseille(hw.vram) : null;
  /** « Toutes », « Processeur » ou « N couches » : le nombre de couches d'un
   *  modèle varie (24 à 80), un pourcentage sur une base supposée mentait. */
  const couchesLabel =
    cfg.gpu_layers === -1
      ? "Toutes"
      : cfg.gpu_layers === 0
        ? "Processeur"
        : `${cfg.gpu_layers} couches`;

  return (
    <div className="perf-panel">
      {/* ── Header ── */}
      <div className="perf-header">
        <div>
          <div className="perf-title">
            <Icon name="speed" size={15} /> Moteur d'Inférence
          </div>
          <div className="perf-subtitle">
            Configure comment le modèle est exécuté sur ta machine
          </div>
        </div>
        <div className="perf-save-badge">
          {saving ? (
            <span className="perf-saving">
              <Icon name="models" size={15} /> Sauvegarde…
            </span>
          ) : saved ? (
            <span className="perf-saved">
              <Icon name="check" size={15} /> Sauvegardé
            </span>
          ) : null}
        </div>
      </div>

      {/* ── Hardware info bar ── */}
      {hw && (
        <div className="perf-hw-bar">
          <div className="perf-hw-chip">
            <Icon name="server" size={15} />
            <span>{hw.vram.toFixed(1)} Go VRAM</span>
          </div>
          <div className="perf-hw-chip">
            <Icon name="models" size={15} />
            <span>{hw.ram.toFixed(0)} Go RAM</span>
          </div>
          <div className="perf-hw-chip">
            <Icon name="memory" size={15} />
            <span>{hw.cores} cœurs</span>
          </div>
          <div className="perf-hw-chip perf-hw-active" title="Profil en cours">
            <span className="perf-hw-icon">
              <Icon name="cpu" size={14} />
            </span>
            <span>{nomDuProfil(cfg.profile)}</span>
          </div>
        </div>
      )}

      {loadError && (
        <div className="perf-error" role="alert">
          <Icon name="warning" size={15} /> {loadError}
        </div>
      )}

      {/* ── Profile cards ── */}
      <div className="perf-section-label">Profil de base</div>
      <div className="perf-profiles" role="radiogroup" aria-label="Profil de base">
        {PROFILES.map((p) => {
          const isActive = cfg.profile === p.id;
          const marque = p.id === conseille ? "Conseillé pour votre carte" : p.badge;
          return (
            <button
              type="button"
              key={p.id}
              role="radio"
              aria-checked={isActive}
              className={`perf-card${isActive ? " perf-card-active" : ""}`}
              onClick={() => void applyProfile(p.id)}
            >
              <div className="perf-card-head">
                <span className="perf-card-icon">
                  <Icon name={p.icon} size={18} />
                </span>
                {isActive && (
                  <span className="perf-card-check" aria-hidden="true">
                    <Icon name="check" size={14} />
                  </span>
                )}
              </div>
              <div className="perf-card-label">{p.label}</div>
              <div className="perf-card-tagline">{p.tagline}</div>
              <ul className="perf-card-details">
                {p.details.map((d) => (
                  <li key={d}>{d}</li>
                ))}
              </ul>
              {marque && (
                <span
                  className={`perf-card-badge${p.id === conseille ? " perf-card-badge-reco" : ""}`}
                >
                  {marque}
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* ── Custom profile notice ── */}
      {(cfg.profile === "custom" || cfg.profile in ANCIENS_PROFILS) && (
        <div className="perf-custom-notice">
          {cfg.profile === "custom"
            ? "Profil personnalisé — les réglages ci-dessous s'appliquent directement"
            : `Ancien profil « ${nomDuProfil(cfg.profile)} » — choisissez Automatique pour laisser le moteur ajuster le contexte à votre carte`}
        </div>
      )}

      {/* ── Expert toggle ── */}
      <button
        type="button"
        className={`perf-expert-toggle${showExpert ? " perf-expert-toggle-open" : ""}`}
        onClick={() => setShowExpert((v) => !v)}
      >
        <span>{showExpert ? "▼" : "▶"} Réglages avancés</span>
        <span className="perf-expert-summary">
          KV {cfg.kv_cache_type.toUpperCase()} · {contexteLisible(cfg.context_length)} ctx ·{" "}
          {cfg.gpu_layers === -1
            ? "GPU max"
            : cfg.gpu_layers === 0
              ? "CPU only"
              : `${cfg.gpu_layers} layers`}
        </span>
      </button>

      {showExpert && (
        <div className="perf-expert-panel">
          {/* KV Cache Type */}
          <div className="perf-row">
            <div className="perf-row-left">
              <div className="perf-row-label">
                <Icon name="archive" size={15} /> Compression KV Cache
              </div>
              <div className="perf-row-hint">
                La mémoire de la conversation, en VRAM. Q8 la divise par deux sans perte visible :
                deux fois plus de contexte sur la même carte. Q4 la divise par quatre, avec un peu
                de précision en moins sur les longs échanges. La vitesse d'écriture ne change
                presque pas.
              </div>
            </div>
            <div className="perf-kv-btns">
              {KV_OPTIONS.map((opt) => (
                <button
                  key={opt.value}
                  type="button"
                  className={`perf-kv-btn${cfg.kv_cache_type === opt.value ? " perf-kv-btn-active" : ""}`}
                  style={
                    cfg.kv_cache_type === opt.value
                      ? { borderColor: opt.color, color: opt.color }
                      : {}
                  }
                  onClick={() => patch({ kv_cache_type: opt.value })}
                  title={opt.desc}
                >
                  {opt.label}
                  <span className="perf-kv-desc">{opt.desc}</span>
                </button>
              ))}
            </div>
          </div>

          {/* GPU Layers */}
          <div className="perf-row">
            <div className="perf-row-left">
              <div className="perf-row-label">Couches déportées sur le GPU</div>
              <div className="perf-row-hint">
                {cfg.gpu_layers === -1
                  ? "Maximum — toutes les couches sur GPU"
                  : cfg.gpu_layers === 0
                    ? "CPU uniquement — aucune couche sur GPU"
                    : `${cfg.gpu_layers} couches sur GPU, reste en RAM`}
              </div>
            </div>
            <div className="perf-slider-wrap">
              <div className="perf-slider-labels">
                <span>Processeur</span>
                <span className="perf-slider-pct">{couchesLabel}</span>
                <span>Tout le GPU</span>
              </div>
              <input
                type="range"
                className="perf-slider"
                min={0}
                max={101}
                value={cfg.gpu_layers === -1 ? 101 : cfg.gpu_layers}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  patch({ gpu_layers: v >= 100 ? -1 : v });
                }}
              />
              <div className="perf-slider-endpoints">
                <button
                  type="button"
                  className="perf-mini-btn"
                  onClick={() => patch({ gpu_layers: 0 })}
                >
                  CPU seul
                </button>
                <button
                  type="button"
                  className="perf-mini-btn"
                  onClick={() => patch({ gpu_layers: -1 })}
                >
                  Tout GPU
                </button>
              </div>
            </div>
          </div>

          {/* Context Length */}
          <div className="perf-row">
            <div className="perf-row-left">
              <div className="perf-row-label">Fenêtre de contexte</div>
              <div className="perf-row-hint">
                Ce que le modèle garde en tête : messages, fichiers lus, outils. Auto = toute la
                mémoire vidéo laissée libre par le modèle. Plus grand = plus de VRAM ; si elle
                manque, des couches passent sur le processeur et tout ralentit.
              </div>
            </div>
            <div className="perf-ctx-btns">
              {CTX_PRESETS.map((p) => (
                <button
                  key={p.value}
                  type="button"
                  className={`perf-ctx-btn${cfg.context_length === p.value ? " perf-ctx-btn-active" : ""}`}
                  onClick={() => patch({ context_length: p.value })}
                >
                  {p.label}
                </button>
              ))}
            </div>
          </div>

          {/* Flash Attention + mmap */}
          <div className="perf-row perf-row-toggles">
            <div className="perf-toggle-item">
              <button
                type="button"
                className={`perf-toggle${cfg.flash_attention ? " perf-toggle-on" : ""}`}
                onClick={() => patch({ flash_attention: !cfg.flash_attention })}
              >
                {cfg.flash_attention ? "ON" : "OFF"}
              </button>
              <div>
                <div className="perf-row-label">
                  <Icon name="speed" size={15} /> Flash Attention
                </div>
                <div className="perf-row-hint">
                  Calcule l'attention par blocs : moins de mémoire, plus rapide sur les longs
                  contextes. Nécessaire pour compresser le cache. À laisser activé.
                </div>
              </div>
            </div>
            <div className="perf-toggle-item">
              <button
                type="button"
                className={`perf-toggle${cfg.use_mmap ? " perf-toggle-on" : ""}`}
                onClick={() => patch({ use_mmap: !cfg.use_mmap })}
              >
                {cfg.use_mmap ? "ON" : "OFF"}
              </button>
              <div>
                <div className="perf-row-label">Chargement par mmap</div>
                <div className="perf-row-hint">Chargement rapide, moins de RAM copiée</div>
              </div>
            </div>
          </div>

          {/* CPU Threads */}
          <div className="perf-row">
            <div className="perf-row-left">
              <div className="perf-row-label">
                <Icon name="memory" size={15} /> Threads CPU
              </div>
              <div className="perf-row-hint">
                {cfg.cpu_threads === 0
                  ? `Auto — ${hw?.cores ?? "?"} cœurs détectés`
                  : `${cfg.cpu_threads} threads manuels`}
              </div>
            </div>
            <div className="perf-slider-wrap">
              <input
                type="range"
                className="perf-slider"
                min={0}
                max={hw?.cores ? hw.cores * 2 : 16}
                value={cfg.cpu_threads}
                onChange={(e) => patch({ cpu_threads: Number(e.target.value) })}
              />
              <div className="perf-slider-endpoints">
                <button
                  type="button"
                  className="perf-mini-btn"
                  onClick={() => patch({ cpu_threads: 0 })}
                >
                  Auto
                </button>
                <span className="perf-slider-pct">
                  {cfg.cpu_threads === 0 ? "Auto" : cfg.cpu_threads}
                </span>
              </div>
            </div>
          </div>

          {/* Batch Size */}
          <div className="perf-row">
            <div className="perf-row-left">
              <div className="perf-row-label">
                <Icon name="models" size={15} /> Taille de Batch
              </div>
              <div className="perf-row-hint">
                Jetons lus d'un coup quand le modèle lit votre message, l'historique et les outils.
                Plus grand = lecture plus rapide des longues demandes, un peu plus de VRAM. Ne
                change pas la vitesse à laquelle il écrit.
              </div>
            </div>
            <div className="perf-ctx-btns">
              {[128, 256, 512, 1024, 2048].map((b) => (
                <button
                  key={b}
                  type="button"
                  className={`perf-ctx-btn${cfg.batch_size === b ? " perf-ctx-btn-active" : ""}`}
                  onClick={() => patch({ batch_size: b })}
                >
                  {b}
                </button>
              ))}
            </div>
          </div>

          {/* Parallel Slots */}
          <div className="perf-row">
            <div className="perf-row-left">
              <div className="perf-row-label">Requêtes en parallèle</div>
              <div className="perf-row-hint">
                Requêtes simultanées (utile pour plusieurs agents)
              </div>
            </div>
            <div className="perf-ctx-btns">
              {[1, 2, 4, 8].map((s) => (
                <button
                  key={s}
                  type="button"
                  className={`perf-ctx-btn${cfg.parallel_slots === s ? " perf-ctx-btn-active" : ""}`}
                  onClick={() => patch({ parallel_slots: s })}
                >
                  {s}
                </button>
              ))}
            </div>
          </div>

          {/* Speculative Decoding */}
          <div className="perf-row perf-row-col">
            <div className="perf-row-left">
              <div className="perf-row-label">
                <Icon name="star" size={15} /> Décodage Spéculatif
              </div>
              <div className="perf-row-hint">
                Un petit modèle "draft" génère des tokens, le grand modèle les valide. ×2 vitesse de
                génération.
              </div>
            </div>
            <input
              type="text"
              className="locaryn-input perf-draft-input"
              placeholder="Chemin vers le modèle draft (ex: models/gemma-2b.gguf)"
              value={cfg.draft_model_path}
              onChange={(e) => patch({ draft_model_path: e.target.value })}
            />
          </div>

          {/* MoE expert offload — run huge Mixture-of-Experts models on a modest GPU */}
          <div className="perf-row perf-row-col">
            <div className="perf-row-left">
              <div className="perf-row-label">
                <Icon name="extensions" size={15} /> Offload experts MoE → CPU
              </div>
              <div className="perf-row-hint">
                Garde les experts d'un modèle MoE (GLM, Qwen3-MoE, DeepSeek) en RAM et l'attention
                sur le GPU. Fait tourner d'énormes modèles sur une petite carte, bien plus vite que
                le streaming SSD.
              </div>
            </div>
            <div className="perf-ctx-btns">
              <button
                type="button"
                className={`perf-ctx-btn${cfg.n_cpu_moe === 0 ? " perf-ctx-btn-active" : ""}`}
                onClick={() => patch({ n_cpu_moe: 0 })}
              >
                Off
              </button>
              <button
                type="button"
                className={`perf-ctx-btn${cfg.n_cpu_moe < 0 ? " perf-ctx-btn-active" : ""}`}
                onClick={() => patch({ n_cpu_moe: -1 })}
                title="Tous les experts sur le CPU (-cmoe)"
              >
                Tout → CPU
              </button>
              <input
                type="number"
                min={0}
                className="locaryn-input perf-moe-input"
                placeholder="N couches"
                value={cfg.n_cpu_moe > 0 ? cfg.n_cpu_moe : ""}
                onChange={(e) => {
                  const n = Number.parseInt(e.target.value, 10);
                  patch({ n_cpu_moe: Number.isFinite(n) && n > 0 ? n : 0 });
                }}
                title="Experts des N premières couches sur le CPU (-ncmoe N)"
              />
            </div>
          </div>

          {/* Distributed inference over RPC — spread layers across machines */}
          <div className="perf-row perf-row-col">
            <div className="perf-row-left">
              <div className="perf-row-label">
                <Icon name="translate" size={15} /> Inférence distribuée (RPC)
              </div>
              <div className="perf-row-hint">
                Répartit les couches du modèle sur plusieurs machines exécutant{" "}
                <code>ggml-rpc-server</code>. Laisse vide pour rester en local.
              </div>
            </div>
            <input
              type="text"
              className="locaryn-input perf-draft-input"
              placeholder="host:port,host:port (ex: 192.168.1.20:50052)"
              value={cfg.rpc_servers}
              onChange={(e) => patch({ rpc_servers: e.target.value })}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
            />
          </div>

          {/* KV Q4 note */}
          {cfg.kv_cache_type === "q4_0" && (
            <div className="perf-turboquant-banner">
              <Icon name="archive" size={15} />
              <div>
                <strong>Cache KV 4-bit</strong> — compression maximale réelle du cache sous
                llama.cpp (<code>-ctk q4_0 -ctv q4_0</code>, ÷4 VRAM), activée avec Flash Attention.
                Léger impact sur la qualité aux très longs contextes.
              </div>
            </div>
          )}
        </div>
      )}

      {/* ── Live summary ── */}
      <div className="perf-summary-bar">
        <div className="perf-summary-item">
          <span className="perf-summary-label">Cache KV</span>
          <span
            className="perf-summary-val"
            style={{ color: KV_OPTIONS.find((k) => k.value === cfg.kv_cache_type)?.color }}
          >
            {cfg.kv_cache_type.toUpperCase()}
          </span>
        </div>
        <div className="perf-summary-sep" />
        <div className="perf-summary-item">
          <span className="perf-summary-label">GPU</span>
          <span className="perf-summary-val">
            {cfg.gpu_layers === -1 ? "Max" : cfg.gpu_layers === 0 ? "OFF" : `${cfg.gpu_layers}L`}
          </span>
        </div>
        <div className="perf-summary-sep" />
        <div className="perf-summary-item">
          <span className="perf-summary-label">Contexte</span>
          <span className="perf-summary-val">{contexteLisible(cfg.context_length)}</span>
        </div>
        <div className="perf-summary-sep" />
        <div className="perf-summary-item">
          <span className="perf-summary-label">Flash Attn</span>
          <span
            className="perf-summary-val"
            style={{ color: cfg.flash_attention ? "var(--accent)" : "var(--text-faint)" }}
          >
            {cfg.flash_attention ? "ON" : "OFF"}
          </span>
        </div>
        <div className="perf-summary-sep" />
        <div className="perf-summary-item">
          <span className="perf-summary-label">Batch</span>
          <span className="perf-summary-val">{cfg.batch_size}</span>
        </div>
        {cfg.draft_model_path && (
          <>
            <div className="perf-summary-sep" />
            <div className="perf-summary-item">
              <span className="perf-summary-label">Spéculatif</span>
              <Icon name="check" size={15} />
            </div>
          </>
        )}
      </div>

      <p className="perf-restart-hint">
        <Icon name="warning" size={15} /> Les modifications s'appliquent au prochain redémarrage du
        moteur (nouvelle session ou reload du modèle).
      </p>
    </div>
  );
}
