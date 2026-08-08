import { useCallback, useEffect, useState } from "react";
import { core } from "../lib/core";

export interface ModelParams {
  temperature: number;    // 0.0 – 2.0
  top_p: number;         // 0.0 – 1.0
  top_k: number;         // 0 – 100
  ctx_size: number;      // tokens: 512 – 131072
  max_tokens: number;    // 0 = unlimited
  repeat_penalty: number; // 1.0 – 2.0
  seed: number;          // -1 = random
}

export const DEFAULT_MODEL_PARAMS: ModelParams = {
  temperature: 0.7,
  top_p: 0.95,
  top_k: 40,
  ctx_size: 8192,
  max_tokens: 0,
  repeat_penalty: 1.1,
  seed: -1,
};

type SliderProps = {
  label: string;
  id: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format?: (v: number) => string;
  onChange: (v: number) => void;
};

function Slider({ label, id, value, min, max, step, format, onChange }: SliderProps) {
  const pct = ((value - min) / (max - min)) * 100;
  return (
    <div className="lmc-field">
      <div className="lmc-field-head">
        <label htmlFor={id} className="lmc-label">{label}</label>
        <span className="lmc-value">{format ? format(value) : value}</span>
      </div>
      <div className="lmc-slider-wrap">
        <input
          id={id}
          type="range"
          className="lmc-slider"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          style={{ "--pct": `${pct}%` } as React.CSSProperties}
        />
      </div>
    </div>
  );
}

type Props = {
  /** Current context window size (from the model config), used to compute usage pct. */
  onParamsChange?: (params: ModelParams) => void;
  onClose?: () => void;
};

export function ModelConfigPanel({ onParamsChange, onClose }: Props) {
  const [params, setParams] = useState<ModelParams>(DEFAULT_MODEL_PARAMS);
  const [saved, setSaved] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load from active provider config on mount.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const providers = await core.listProviders();
        const active = providers.find((p) => p.is_active) ?? providers[0];
        if (active?.config) {
          const cfg = active.config as Partial<ModelParams>;
          setParams((prev) => ({ ...prev, ...cfg }));
        }
      } catch {
        // Keep defaults silently.
      }
    })();
    return () => { cancelled = true; void cancelled; };
  }, []);

  const update = useCallback((key: keyof ModelParams, val: number) => {
    setParams((prev) => {
      const next = { ...prev, [key]: val };
      onParamsChange?.(next);
      return next;
    });
    setSaved(false);
  }, [onParamsChange]);

  async function save() {
    setSaving(true);
    setError(null);
    try {
      await core.updateProviderModelParams(params);
      setSaved(true);
      setTimeout(() => setSaved(false), 1800);
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setSaving(false);
    }
  }

  function reset() {
    setParams(DEFAULT_MODEL_PARAMS);
    onParamsChange?.(DEFAULT_MODEL_PARAMS);
    setSaved(false);
  }

  return (
    <aside className="lmc-panel">
      <div className="lmc-header" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <span className="lmc-title">⚙️ Paramètres du Modèle</span>
        <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
          <button
            type="button"
            className="lmc-reset-btn"
            onClick={reset}
            title="Réinitialiser les paramètres"
          >
            Reset
          </button>
          {onClose && (
            <button
              type="button"
              className="locaryn-icon-btn"
              onClick={onClose}
              title="Fermer ce panneau"
              style={{ fontSize: "14px", padding: "2px 6px" }}
            >
              ✕
            </button>
          )}
        </div>
      </div>

      <div className="lmc-body">
        <Slider
          id="lmc-temperature"
          label="Temperature"
          value={params.temperature}
          min={0}
          max={2}
          step={0.01}
          format={(v) => v.toFixed(2)}
          onChange={(v) => update("temperature", v)}
        />

        <Slider
          id="lmc-top-p"
          label="Top-P"
          value={params.top_p}
          min={0}
          max={1}
          step={0.01}
          format={(v) => v.toFixed(2)}
          onChange={(v) => update("top_p", v)}
        />

        <Slider
          id="lmc-top-k"
          label="Top-K"
          value={params.top_k}
          min={0}
          max={100}
          step={1}
          onChange={(v) => update("top_k", v)}
        />

        <Slider
          id="lmc-repeat-penalty"
          label="Repeat penalty"
          value={params.repeat_penalty}
          min={1}
          max={2}
          step={0.01}
          format={(v) => v.toFixed(2)}
          onChange={(v) => update("repeat_penalty", v)}
        />

        <div className="lmc-divider" />

        <Slider
          id="lmc-ctx-size"
          label="Context window"
          value={params.ctx_size}
          min={512}
          max={131072}
          step={512}
          format={(v) => v >= 1024 ? `${(v / 1024).toFixed(0)}k` : `${v}`}
          onChange={(v) => update("ctx_size", v)}
        />

        <Slider
          id="lmc-max-tokens"
          label="Max new tokens"
          value={params.max_tokens}
          min={0}
          max={16384}
          step={64}
          format={(v) => v === 0 ? "∞" : `${v}`}
          onChange={(v) => update("max_tokens", v)}
        />

        <div className="lmc-divider" />

        <div className="lmc-field">
          <div className="lmc-field-head">
            <label htmlFor="lmc-seed" className="lmc-label">Seed</label>
            <span className="lmc-value lmc-value-mono">
              {params.seed === -1 ? "random" : params.seed}
            </span>
          </div>
          <div className="lmc-seed-row">
            <input
              id="lmc-seed"
              type="number"
              className="lmc-seed-input"
              value={params.seed}
              min={-1}
              max={2147483647}
              onChange={(e) => update("seed", Number(e.target.value))}
            />
            <button
              type="button"
              className="lmc-seed-rand"
              onClick={() => update("seed", Math.floor(Math.random() * 2147483647))}
              title="Random seed"
            >
              🎲
            </button>
            <button
              type="button"
              className="lmc-seed-rand"
              onClick={() => update("seed", -1)}
              title="Set to random"
            >
              ∞
            </button>
          </div>
        </div>
      </div>

      <div className="lmc-footer">
        {error && <div className="lmc-error">{error}</div>}
        <button
          type="button"
          className="lmc-save-btn"
          onClick={save}
          disabled={saving}
        >
          {saving ? "Saving…" : saved ? "Saved ✓" : "Apply"}
        </button>
      </div>
    </aside>
  );
}
