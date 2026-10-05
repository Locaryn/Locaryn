import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useRef, useState } from "react";
import { SessionTrustControl } from "../components/SessionTrustControl";
import { useContextStatus } from "../hooks/useContextStatus";
import { type ModelRecommendation, core } from "../lib/core";

function formatContext(v: number): string {
  if (v === 0) return "Auto";
  return v >= 1024 ? `${Math.round(v / 1024)}k` : `${v}`;
}

export interface ModelParams {
  temperature: number; // 0.0 – 2.0
  top_p: number; // 0.0 – 1.0
  top_k: number; // 0 – 100
  ctx_size: number; // tokens: 512 – 131072
  max_tokens: number; // 0 = unlimited
  repeat_penalty: number; // 1.0 – 2.0
  seed: number; // -1 = random
  min_p: number; // 0.0 – 1.0
}

export const DEFAULT_MODEL_PARAMS: ModelParams = {
  temperature: 0.7,
  top_p: 0.95,
  top_k: 40,
  ctx_size: 8192,
  max_tokens: 0,
  repeat_penalty: 1.1,
  seed: -1,
  min_p: 0.05,
};

/** Les réglages d'échantillonnage ont-ils quitté les défauts ? Même règle que
 *  le service (`echantillonnage_modifie`) : sinon, la recommandation du modèle
 *  s'applique. */
function echantillonnageModifie(p: ModelParams): boolean {
  const d = DEFAULT_MODEL_PARAMS;
  const differe = (a: number, b: number) => Math.abs(a - b) > 1e-4;
  return (
    differe(p.temperature, d.temperature) ||
    differe(p.top_p, d.top_p) ||
    p.top_k !== d.top_k ||
    differe(p.repeat_penalty, d.repeat_penalty) ||
    differe(p.min_p, d.min_p)
  );
}

/** Ce qui part vraiment vers le modèle : la recommandation tant que la
 *  personne n'a rien changé. */
function effectifs(p: ModelParams, r: ModelRecommendation | null): ModelParams {
  if (!r || echantillonnageModifie(p)) return p;
  return {
    ...p,
    temperature: r.temperature ?? p.temperature,
    top_p: r.top_p ?? p.top_p,
    top_k: r.top_k ?? p.top_k,
    min_p: r.min_p ?? p.min_p,
    repeat_penalty: r.repeat_penalty ?? p.repeat_penalty,
  };
}

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
        <label htmlFor={id} className="lmc-label">
          {label}
        </label>
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
  /** La conversation ouverte : c'est elle dont on regle les permissions. */
  sessionId?: string | null;
};

export function ModelConfigPanel({ onParamsChange, onClose, sessionId }: Props) {
  const [params, setParams] = useState<ModelParams>(DEFAULT_MODEL_PARAMS);
  const [saved, setSaved] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // La fenêtre de contexte a une seule source (voir `useContextStatus`) : celle de
  // la configuration d'inférence, la même que le profil du moteur et la jauge du
  // chat. Le panneau gardait sa propre copie, qui divergeait.
  const {
    status: contextStatus,
    pending: contextPending,
    refresh: refreshContext,
  } = useContextStatus();
  const [contextEdit, setContextEdit] = useState<number | null>(null);
  const [reco, setReco] = useState<ModelRecommendation | null>(null);
  const affiches = effectifs(params, reco);
  const recoAppliquee = reco !== null && !echantillonnageModifie(params);
  const [reloading, setReloading] = useState(false);
  const contextTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Load from active provider config on mount.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const providers = await core.listProviders();
        const active = providers.find((p) => p.is_active) ?? providers[0];
        if (active?.config && !cancelled) {
          const cfg = active.config as Partial<ModelParams>;
          setParams((prev) => ({ ...prev, ...cfg }));
        }
        const r = await core.modelRecommendation();
        if (!cancelled) setReco(r);
      } catch {
        // Keep defaults silently.
      }
    })();
    return () => {
      cancelled = true;
      void cancelled;
    };
  }, []);

  const update = useCallback(
    (key: keyof ModelParams, val: number) => {
      setParams((prev) => {
        const next = { ...effectifs(prev, reco), [key]: val };
        onParamsChange?.(next);
        return next;
      });
      setSaved(false);
    },
    [onParamsChange, reco],
  );

  /** Régler la fenêtre : on écrit la configuration d'inférence, et le profil du
   *  moteur reste ce qu'il est — seule la fenêtre change. */
  function changeContext(value: number) {
    setContextEdit(value);
    if (contextTimer.current) clearTimeout(contextTimer.current);
    contextTimer.current = setTimeout(async () => {
      try {
        const cfg = await core.getInferenceConfig();
        await core.setInferenceConfig({ ...cfg, context_length: value });
        window.dispatchEvent(new Event("locaryn:inference-config-changed"));
        await refreshContext();
      } catch (e) {
        setError(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setContextEdit(null);
      }
    }, 500);
  }

  /** Décharger le modèle : llama-server se relance au prochain message, avec la
   *  fenêtre réglée. Le contexte est dimensionné au chargement, pas à la volée. */
  async function reloadModel() {
    setReloading(true);
    setError(null);
    try {
      await core.ejectChatModel();
      window.dispatchEvent(new Event("locaryn:model-ejected"));
      await refreshContext();
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setReloading(false);
    }
  }

  async function save() {
    setSaving(true);
    setError(null);
    try {
      // La fenêtre est enregistrée à part (ci-dessus) : on renvoie celle de la
      // configuration pour ne pas la réécrire avec une ancienne valeur.
      await core.updateProviderModelParams({
        ...params,
        ctx_size: contextStatus?.configured ?? params.ctx_size,
      });
      // La jauge du chat suit la fenetre nouvellement appliquee, sans
      // attendre une reouverture de conversation.
      window.dispatchEvent(new CustomEvent("locaryn:model-params-applied", { detail: { params } }));
      onParamsChange?.(params);
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

  /** Rendre la main à la recommandation : seul l'échantillonnage revient aux
   *  défauts, la longueur de réponse et la graine restent. */
  function restoreRecommendation() {
    const d = DEFAULT_MODEL_PARAMS;
    const next = {
      ...params,
      temperature: d.temperature,
      top_p: d.top_p,
      top_k: d.top_k,
      repeat_penalty: d.repeat_penalty,
      min_p: d.min_p,
    };
    setParams(next);
    onParamsChange?.(next);
    setSaved(false);
  }

  return (
    <aside className="lmc-panel">
      <div
        className="lmc-header"
        style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}
      >
        <span className="lmc-title">
          <Icon name="settings" size={15} /> Paramètres du Modèle
        </span>
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
              <Icon name="close" size={16} />
            </button>
          )}
        </div>
      </div>

      <div className="lmc-body">
        {reco && (
          <RecommendationBanner
            reco={reco}
            applied={recoAppliquee}
            onRestore={restoreRecommendation}
          />
        )}
        <Slider
          id="lmc-temperature"
          label="Temperature"
          value={affiches.temperature}
          min={0}
          max={2}
          step={0.01}
          format={(v) => v.toFixed(2)}
          onChange={(v) => update("temperature", v)}
        />

        <Slider
          id="lmc-top-p"
          label="Top-P"
          value={affiches.top_p}
          min={0}
          max={1}
          step={0.01}
          format={(v) => v.toFixed(2)}
          onChange={(v) => update("top_p", v)}
        />

        <Slider
          id="lmc-top-k"
          label="Top-K"
          value={affiches.top_k}
          min={0}
          max={100}
          step={1}
          onChange={(v) => update("top_k", v)}
        />

        <Slider
          id="lmc-repeat-penalty"
          label="Repeat penalty"
          value={affiches.repeat_penalty}
          min={1}
          max={2}
          step={0.01}
          format={(v) => v.toFixed(2)}
          onChange={(v) => update("repeat_penalty", v)}
        />

        <Slider
          id="lmc-min-p"
          label="Min-P"
          value={affiches.min_p}
          min={0}
          max={1}
          step={0.01}
          format={(v) => v.toFixed(2)}
          onChange={(v) => update("min_p", v)}
        />

        <div className="lmc-divider" />

        {sessionId && (
          <div className="lmc-field">
            <div className="lmc-field-head">
              <span className="lmc-label">Permissions de la conversation</span>
            </div>
            <SessionTrustControl sessionId={sessionId} />
          </div>
        )}

        <div className="lmc-divider" />

        <Slider
          id="lmc-ctx-size"
          label="Fenêtre de contexte"
          value={Math.min(
            contextEdit ?? contextStatus?.configured ?? params.ctx_size,
            contextStatus?.cap ?? 131072,
          )}
          min={0}
          max={contextStatus?.cap ?? 131072}
          step={1024}
          format={formatContext}
          onChange={changeContext}
        />
        <p className="lmc-ctx-cap">
          {contextStatus?.running
            ? `Chargée dans le moteur : ${formatContext(contextStatus.running)}. `
            : "Aucun modèle chargé : ce réglage servira au prochain chargement. "}
          {contextStatus?.cap ? `Plafond du modèle : ${formatContext(contextStatus.cap)}. ` : ""}
          {contextStatus?.configured === 0
            ? "Tout à gauche, Auto : le moteur prend la mémoire vidéo que le modèle laisse libre. "
            : ""}
          Ce réglage ne change pas le profil du moteur d'inférence.
        </p>
        {contextPending && contextStatus?.running != null && (
          <div className="lmc-ctx-pending" role="status">
            <span>
              Le moteur tourne avec {formatContext(contextStatus.running)} ;{" "}
              {formatContext(contextStatus.configured)} s'appliquera au prochain chargement.
            </span>
            <button
              type="button"
              className="lmc-save-btn"
              disabled={reloading}
              onClick={() => void reloadModel()}
            >
              {reloading ? "Déchargement…" : "Recharger le modèle maintenant"}
            </button>
          </div>
        )}

        <Slider
          id="lmc-max-tokens"
          label="Max new tokens"
          value={params.max_tokens}
          min={0}
          max={16384}
          step={64}
          format={(v) => (v === 0 ? "∞" : `${v}`)}
          onChange={(v) => update("max_tokens", v)}
        />

        <div className="lmc-divider" />

        <div className="lmc-field">
          <div className="lmc-field-head">
            <label htmlFor="lmc-seed" className="lmc-label">
              Seed
            </label>
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
              <Icon name="refresh" size={14} />
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
        <button type="button" className="lmc-save-btn" onClick={save} disabled={saving}>
          {saving ? "Enregistrement…" : saved ? "Enregistré" : "Appliquer"}
        </button>
      </div>
    </aside>
  );
}

function formatReco(r: ModelRecommendation): string {
  const parts: string[] = [];
  if (r.temperature != null) parts.push(`température ${r.temperature}`);
  if (r.top_p != null) parts.push(`top-p ${r.top_p}`);
  if (r.top_k != null) parts.push(`top-k ${r.top_k}`);
  if (r.min_p != null) parts.push(`min-p ${r.min_p}`);
  if (r.repeat_penalty != null)
    parts.push(
      r.repeat_penalty <= 1 ? "sans pénalité de répétition" : `pénalité ${r.repeat_penalty}`,
    );
  return parts.join(", ");
}

/** D'où viennent les valeurs affichées : de la recommandation des créateurs du
 *  modèle, ou des réglages de la personne. */
function RecommendationBanner({
  reco,
  applied,
  onRestore,
}: {
  reco: ModelRecommendation;
  applied: boolean;
  onRestore: () => void;
}) {
  const source = reco.source || "les créateurs du modèle";
  return (
    <div className={`lmc-reco${applied ? " is-applied" : ""}`} role="status">
      <span>
        {applied ? `Réglages recommandés par ${source} : ` : `${source} recommande : `}
        {formatReco(reco)}.
      </span>
      {!applied && (
        <button type="button" className="lmc-reco-btn" onClick={onRestore}>
          Revenir aux réglages recommandés
        </button>
      )}
    </div>
  );
}
