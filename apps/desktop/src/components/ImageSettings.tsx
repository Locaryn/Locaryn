import { useEffect, useState } from "react";
import { core, IMAGE_QUALITIES, type ImageDefaults } from "../lib/core";

const VRAM_MODES: { id: string; label: string; hint: string }[] = [
  { id: "gpu", label: "GPU", hint: "Tout en VRAM — le plus rapide" },
  { id: "auto", label: "Auto", hint: "Réparti selon la VRAM libre (recommandé)" },
  { id: "lowvram", label: "Low VRAM", hint: "Poids en RAM — tourne sur petit GPU" },
];

/**
 * Defaults used by every image generation that doesn't override them —
 * notably the `/image` slash command, whose settings were previously invisible.
 * Picking "Maximale" here means one-shot 1024px renders without touching
 * anything per prompt.
 */
export function ImageSettings() {
  const [cfg, setCfg] = useState<ImageDefaults | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    let cancelled = false;
    core.getImageDefaults()
      .then((c) => { if (!cancelled) setCfg(c); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, []);

  async function patch(next: Partial<ImageDefaults>) {
    if (!cfg) return;
    // Resolve the resolution here as well as in the backend, so the UI reflects
    // the choice immediately instead of waiting on a round-trip.
    const preset = next.quality ? IMAGE_QUALITIES.find((q) => q.id === next.quality) : undefined;
    const merged: ImageDefaults = {
      ...cfg,
      ...next,
      ...(preset ? { width: preset.px, height: preset.px } : {}),
    };
    setCfg(merged);
    try {
      await core.setImageDefaults(merged);
      setSaved(true);
      window.setTimeout(() => setSaved(false), 1400);
    } catch {
      // Keep the optimistic value; the next save will retry.
    }
  }

  if (!cfg) return <p className="lochor-field-hint">Chargement…</p>;

  return (
    <div>
      <div className="lochor-field">
        <label className="lochor-field-label">
          Qualité par défaut
          {saved && <span style={{ marginLeft: 8, color: "var(--accent)", fontSize: "var(--text-xs)" }}>enregistré ✓</span>}
        </label>
        <p className="lochor-field-hint">
          Appliquée à <code>/image</code> et au bouton Créer quand aucune valeur n'est précisée.
        </p>
        <div className="lochor-imgset-grid" style={{ marginTop: 10 }}>
          {IMAGE_QUALITIES.map((q) => (
            <button
              key={q.id}
              type="button"
              className={`lochor-imgset-item${cfg.quality === q.id ? " lochor-active" : ""}`}
              onClick={() => patch({ quality: q.id })}
            >
              <span className="lochor-imgset-px">{q.px}px</span>
              <span className="lochor-imgset-label">{q.label}</span>
              <span className="lochor-imgset-hint">{q.hint}</span>
            </button>
          ))}
        </div>
        <p className="lochor-field-hint" style={{ marginTop: 8 }}>
          Résolution active : <strong>{cfg.width}×{cfg.height}</strong>
          {cfg.quality === "custom" && " (personnalisée)"}
        </p>
      </div>

      <div className="lochor-field" style={{ marginTop: 20 }}>
        <label className="lochor-field-label">Mémoire (dispatch RAM/VRAM)</label>
        <div className="lochor-imgset-row" style={{ marginTop: 8 }}>
          {VRAM_MODES.map((m) => (
            <button
              key={m.id}
              type="button"
              className={`perf-ctx-btn${cfg.vram_mode === m.id ? " perf-ctx-btn-active" : ""}`}
              onClick={() => patch({ vram_mode: m.id })}
              title={m.hint}
            >
              {m.label}
            </button>
          ))}
        </div>
        <p className="lochor-field-hint" style={{ marginTop: 6 }}>
          {VRAM_MODES.find((m) => m.id === cfg.vram_mode)?.hint}
        </p>
      </div>

      <div className="lochor-field" style={{ marginTop: 20 }}>
        <label className="lochor-field-label">Étapes (steps)</label>
        <p className="lochor-field-hint">
          0 = laisser le modèle décider. Les modèles turbo sont bornés automatiquement
          (≈8 étapes) : monter plus haut les ralentit sans gain.
        </p>
        <input
          type="number"
          min={0}
          max={50}
          className="lochor-input"
          style={{ width: 120, marginTop: 8 }}
          value={cfg.steps}
          onChange={(e) => patch({ steps: Math.max(0, Math.min(50, Number(e.target.value) || 0)) })}
        />
      </div>

      <div className="lochor-field" style={{ marginTop: 20 }}>
        <label className="lochor-field-label">Variantes par génération</label>
        <p className="lochor-field-hint">
          Produit plusieurs images d'un coup pour en choisir une. Le modèle n'est chargé
          qu'une fois : chaque variante supplémentaire coûte nettement moins cher qu'une
          génération séparée. Utile quand un prompt demande plusieurs essais.
        </p>
        <div className="lochor-imgset-row" style={{ marginTop: 8 }}>
          {[1, 2, 3, 4, 6, 8].map((n) => (
            <button
              key={n}
              type="button"
              className={`perf-ctx-btn${cfg.variants === n ? " perf-ctx-btn-active" : ""}`}
              onClick={() => patch({ variants: n })}
            >
              {n}
            </button>
          ))}
        </div>
        <p className="lochor-field-hint" style={{ marginTop: 6 }}>
          {cfg.variants > 1
            ? `≈ ${cfg.variants} images par demande — comptez environ ${Math.round(
                (100 * (1 - (1 + (cfg.variants - 1) * 0.69) / cfg.variants)),
              )} % de temps gagné par rapport à ${cfg.variants} générations séparées.`
            : "Une seule image par demande."}
        </p>
      </div>

      <div className="lochor-field" style={{ marginTop: 20 }}>
        <label className="lochor-field-label">Prompt négatif par défaut</label>
        <p className="lochor-field-hint">Ce que les images doivent éviter (facultatif).</p>
        <input
          className="lochor-input"
          style={{ marginTop: 8 }}
          placeholder="flou, filigrane, difforme…"
          value={cfg.negative_prompt}
          onChange={(e) => setCfg({ ...cfg, negative_prompt: e.target.value })}
          onBlur={(e) => patch({ negative_prompt: e.target.value })}
        />
      </div>
    </div>
  );
}
