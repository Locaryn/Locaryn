import { useCallback, useEffect, useRef, useState } from "react";
import { core, type InferenceConfig, type InferenceProfile, type KvCacheType } from "../lib/core";

// ── Types ─────────────────────────────────────────────────────────────────────

interface ProfileCard {
  id: InferenceProfile;
  icon: string;
  label: string;
  tagline: string;
  details: string[];
  badge?: string;
  badgeColor?: string;
}

const PROFILES: ProfileCard[] = [
  {
    id: "eco",
    icon: "🌱",
    label: "Économe",
    tagline: "CPU uniquement, mémoire minimale",
    details: ["0 couches GPU", "Cache FP16 standard", "Contexte 4K tokens"],
  },
  {
    id: "balanced",
    icon: "⚡",
    label: "Équilibré",
    tagline: "Mix GPU/CPU, bon compromis",
    details: ["Toutes les couches GPU", "Cache Q8 (÷2 VRAM)", "Contexte 8K tokens", "Flash Attention"],
  },
  {
    id: "performance",
    icon: "🚀",
    label: "Performance",
    tagline: "GPU au maximum, contexte long",
    details: ["Toutes les couches GPU", "Cache Q8 compressé", "Contexte 16K tokens", "Flash Attention"],
  },
  {
    id: "turbo",
    icon: "🔥",
    label: "Turbo",
    tagline: "KV Q4 + GPU max + contexte 32K",
    details: ["Toutes les couches GPU", "Cache Q4 (÷4 VRAM)", "Contexte 32K tokens", "Flash Attention", "Batch 1024"],
    badge: "Recommandé",
    badgeColor: "rgba(111, 156, 127, 0.9)",
  },
  {
    id: "longctx",
    icon: "↔",
    label: "Contexte long",
    tagline: "Cache KV 4-bit — max de contexte à VRAM égale",
    details: ["Toutes les couches GPU", "Cache KV Q4 (÷4 VRAM)", "Contexte étendu", "Flash Attention", "llama.cpp géré"],
    badge: "Contexte max",
    badgeColor: "rgba(212, 160, 58, 0.9)",
  },
];

const KV_OPTIONS: { value: KvCacheType; label: string; desc: string; color: string }[] = [
  { value: "f16", label: "FP16", desc: "Standard", color: "#6a6d68" },
  { value: "q8_0", label: "Q8", desc: "÷2 VRAM", color: "#6f9c7f" },
  { value: "q4_0", label: "Q4", desc: "÷4 VRAM (max réel)", color: "#d4a03a" },
];

const CTX_PRESETS = [
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
  const [expandedProfile, setExpandedProfile] = useState<string | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    core.getInferenceConfig().then(setCfg);
    core.checkHardware().then((h) => setHw({ vram: h.total_vram_gb, ram: h.total_ram_gb, cores: h.cpu_cores ?? 4 }));
  }, []);

  const autoSave = useCallback((newCfg: InferenceConfig) => {
    if (saveTimer.current) clearTimeout(saveTimer.current);
    setSaved(false);
    saveTimer.current = setTimeout(async () => {
      setSaving(true);
      try {
        await core.setInferenceConfig(newCfg);
        setSaved(true);
        setTimeout(() => setSaved(false), 2500);
      } finally {
        setSaving(false);
      }
    }, 600);
  }, []);

  const patch = useCallback((delta: Partial<InferenceConfig>) => {
    setCfg((prev) => {
      if (!prev) return prev;
      const next = { ...prev, ...delta };
      // Any manual change → custom profile (unless we just applied a preset)
      if (!("profile" in delta)) next.profile = "custom";
      autoSave(next);
      return next;
    });
  }, [autoSave]);

  const applyProfile = useCallback(async (id: InferenceProfile) => {
    const preset = await core.getProfilePreset(id);
    // Merge: keep draft_model_path from existing config
    setCfg((prev) => {
      const next = { ...preset, draft_model_path: prev?.draft_model_path ?? "" };
      autoSave(next);
      return next;
    });
  }, [autoSave]);

  if (!cfg) return <div className="perf-loading">Chargement…</div>;

  const gpuPct = cfg.gpu_layers === -1 ? 100 : cfg.gpu_layers === 0 ? 0 : Math.min(100, Math.round((cfg.gpu_layers / 80) * 100));

  return (
    <div className="perf-panel">
      {/* ── Header ── */}
      <div className="perf-header">
        <div>
          <div className="perf-title">⚡ Moteur d'Inférence</div>
          <div className="perf-subtitle">
            Configure comment le modèle est exécuté sur ta machine
          </div>
        </div>
        <div className="perf-save-badge">
          {saving ? <span className="perf-saving">💾 Sauvegarde…</span> : saved ? <span className="perf-saved">✓ Sauvegardé</span> : null}
        </div>
      </div>

      {/* ── Hardware info bar ── */}
      {hw && (
        <div className="perf-hw-bar">
          <div className="perf-hw-chip">
            <span className="perf-hw-icon">🖥️</span>
            <span>{hw.vram.toFixed(1)} Go VRAM</span>
          </div>
          <div className="perf-hw-chip">
            <span className="perf-hw-icon">💾</span>
            <span>{hw.ram.toFixed(0)} Go RAM</span>
          </div>
          <div className="perf-hw-chip">
            <span className="perf-hw-icon">🧠</span>
            <span>{hw.cores} cœurs</span>
          </div>
          <div className="perf-hw-chip perf-hw-active">
            <span className="perf-hw-icon">🏷️</span>
            <span>{cfg.profile}</span>
          </div>
        </div>
      )}

      {/* ── Profile cards ── */}
      <div className="perf-section-label">Profil de base</div>
      <div className="perf-profiles">
        {PROFILES.map((p) => {
          const isActive = cfg.profile === p.id;
          const isExpanded = expandedProfile === p.id;
          return (
            <div
              key={p.id}
              className={`perf-card${isActive ? " perf-card-active" : ""}`}
              onClick={() => {
                applyProfile(p.id);
                setExpandedProfile(isExpanded ? null : p.id);
              }}
            >
              {p.badge && (
                <div className="perf-card-badge" style={{ background: p.badgeColor }}>
                  {p.badge}
                </div>
              )}
              <div className="perf-card-icon">{p.icon}</div>
              <div className="perf-card-label">{p.label}</div>
              <div className="perf-card-tagline">{p.tagline}</div>
              {(isActive || isExpanded) && (
                <ul className="perf-card-details">
                  {p.details.map((d) => <li key={d}>{d}</li>)}
                </ul>
              )}
            </div>
          );
        })}
      </div>

      {/* ── Custom profile notice ── */}
      {cfg.profile === "custom" && (
        <div className="perf-custom-notice">
          🎛️ Profil personnalisé — les réglages ci-dessous s'appliquent directement
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
          KV {cfg.kv_cache_type.toUpperCase()} · {cfg.context_length >= 1024 ? `${Math.round(cfg.context_length / 1024)}K` : cfg.context_length} ctx ·{" "}
          {cfg.gpu_layers === -1 ? "GPU max" : cfg.gpu_layers === 0 ? "CPU only" : `${cfg.gpu_layers} layers`}
        </span>
      </button>

      {showExpert && (
        <div className="perf-expert-panel">

          {/* KV Cache Type */}
          <div className="perf-row">
            <div className="perf-row-left">
              <div className="perf-row-label">🗜️ Compression KV Cache</div>
              <div className="perf-row-hint">Compresse la mémoire de conversation. Q4 = compression réelle max (÷4 VRAM) sous llama.cpp</div>
            </div>
            <div className="perf-kv-btns">
              {KV_OPTIONS.map((opt) => (
                <button
                  key={opt.value}
                  type="button"
                  className={`perf-kv-btn${cfg.kv_cache_type === opt.value ? " perf-kv-btn-active" : ""}`}
                  style={cfg.kv_cache_type === opt.value ? { borderColor: opt.color, color: opt.color } : {}}
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
              <div className="perf-row-label">🎮 Couches GPU (Offloading)</div>
              <div className="perf-row-hint">
                {cfg.gpu_layers === -1 ? "Maximum — toutes les couches sur GPU" : cfg.gpu_layers === 0 ? "CPU uniquement — aucune couche sur GPU" : `${cfg.gpu_layers} couches sur GPU, reste en RAM`}
              </div>
            </div>
            <div className="perf-slider-wrap">
              <div className="perf-slider-labels">
                <span>CPU</span>
                <span className="perf-slider-pct">{gpuPct}%</span>
                <span>GPU max</span>
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
                <button type="button" className="perf-mini-btn" onClick={() => patch({ gpu_layers: 0 })}>CPU seul</button>
                <button type="button" className="perf-mini-btn" onClick={() => patch({ gpu_layers: -1 })}>Tout GPU</button>
              </div>
            </div>
          </div>

          {/* Context Length */}
          <div className="perf-row">
            <div className="perf-row-left">
              <div className="perf-row-label">📏 Fenêtre de Contexte</div>
              <div className="perf-row-hint">Mémoire de la conversation. Plus grand = plus de VRAM</div>
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
                <div className="perf-row-label">⚡ Flash Attention</div>
                <div className="perf-row-hint">-30% VRAM, +vitesse attention</div>
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
                <div className="perf-row-label">💿 mmap Chargement</div>
                <div className="perf-row-hint">Chargement rapide, moins de RAM copiée</div>
              </div>
            </div>
          </div>

          {/* CPU Threads */}
          <div className="perf-row">
            <div className="perf-row-left">
              <div className="perf-row-label">🧠 Threads CPU</div>
              <div className="perf-row-hint">
                {cfg.cpu_threads === 0 ? `Auto — ${hw?.cores ?? "?"} cœurs détectés` : `${cfg.cpu_threads} threads manuels`}
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
                <button type="button" className="perf-mini-btn" onClick={() => patch({ cpu_threads: 0 })}>Auto</button>
                <span className="perf-slider-pct">{cfg.cpu_threads === 0 ? "Auto" : cfg.cpu_threads}</span>
              </div>
            </div>
          </div>

          {/* Batch Size */}
          <div className="perf-row">
            <div className="perf-row-left">
              <div className="perf-row-label">📦 Taille de Batch</div>
              <div className="perf-row-hint">Tokens traités en parallèle. Plus grand = plus rapide mais +VRAM</div>
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
              <div className="perf-row-label">🔀 Slots Parallèles</div>
              <div className="perf-row-hint">Requêtes simultanées (utile pour plusieurs agents)</div>
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
              <div className="perf-row-label">🔮 Décodage Spéculatif</div>
              <div className="perf-row-hint">Un petit modèle "draft" génère des tokens, le grand modèle les valide. ×2 vitesse de génération.</div>
            </div>
            <input
              type="text"
              className="lochor-input perf-draft-input"
              placeholder="Chemin vers le modèle draft (ex: models/gemma-2b.gguf)"
              value={cfg.draft_model_path}
              onChange={(e) => patch({ draft_model_path: e.target.value })}
            />
          </div>

          {/* MoE expert offload — run huge Mixture-of-Experts models on a modest GPU */}
          <div className="perf-row perf-row-col">
            <div className="perf-row-left">
              <div className="perf-row-label">🧩 Offload experts MoE → CPU</div>
              <div className="perf-row-hint">Garde les experts d'un modèle MoE (GLM, Qwen3-MoE, DeepSeek) en RAM et l'attention sur le GPU. Fait tourner d'énormes modèles sur une petite carte, bien plus vite que le streaming SSD.</div>
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
                className="lochor-input perf-moe-input"
                placeholder="N couches"
                value={cfg.n_cpu_moe > 0 ? cfg.n_cpu_moe : ""}
                onChange={(e) => {
                  const n = parseInt(e.target.value, 10);
                  patch({ n_cpu_moe: Number.isFinite(n) && n > 0 ? n : 0 });
                }}
                title="Experts des N premières couches sur le CPU (-ncmoe N)"
              />
            </div>
          </div>

          {/* Distributed inference over RPC — spread layers across machines */}
          <div className="perf-row perf-row-col">
            <div className="perf-row-left">
              <div className="perf-row-label">🌐 Inférence distribuée (RPC)</div>
              <div className="perf-row-hint">Répartit les couches du modèle sur plusieurs machines exécutant <code>ggml-rpc-server</code>. Laisse vide pour rester en local.</div>
            </div>
            <input
              type="text"
              className="lochor-input perf-draft-input"
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
              <span className="perf-tq-icon">🗜️</span>
              <div>
                <strong>Cache KV 4-bit</strong> — compression maximale réelle du cache sous llama.cpp
                (<code>-ctk q4_0 -ctv q4_0</code>, ÷4 VRAM), activée avec Flash Attention.
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
          <span className="perf-summary-val" style={{ color: KV_OPTIONS.find(k => k.value === cfg.kv_cache_type)?.color }}>
            {cfg.kv_cache_type.toUpperCase()}
          </span>
        </div>
        <div className="perf-summary-sep" />
        <div className="perf-summary-item">
          <span className="perf-summary-label">GPU</span>
          <span className="perf-summary-val">{cfg.gpu_layers === -1 ? "Max" : cfg.gpu_layers === 0 ? "OFF" : `${cfg.gpu_layers}L`}</span>
        </div>
        <div className="perf-summary-sep" />
        <div className="perf-summary-item">
          <span className="perf-summary-label">Contexte</span>
          <span className="perf-summary-val">{cfg.context_length >= 1024 ? `${Math.round(cfg.context_length / 1024)}K` : cfg.context_length}</span>
        </div>
        <div className="perf-summary-sep" />
        <div className="perf-summary-item">
          <span className="perf-summary-label">Flash Attn</span>
          <span className="perf-summary-val" style={{ color: cfg.flash_attention ? "#6f9c7f" : "#6a6d68" }}>
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
              <span className="perf-summary-val" style={{ color: "#d4a03a" }}>✓</span>
            </div>
          </>
        )}
      </div>

      <p className="perf-restart-hint">
        ⚠️ Les modifications s'appliquent au prochain redémarrage du moteur (nouvelle session ou reload du modèle).
      </p>
    </div>
  );
}
