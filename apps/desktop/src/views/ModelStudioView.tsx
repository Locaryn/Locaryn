import { useEffect, useState } from "react";
import { core, type InferenceConfig, type LoraAdapter, type RuntimeCapabilities } from "../lib/core";

type Props = {
  onOpenMarketplace?: () => void;
  onOpenSettings?: () => void;
};

/** What can genuinely be done to a model from this app, and what cannot. */
const DOABLE = [
  { icon: "🎯", label: "Adaptateurs LoRA", desc: "Charger des .gguf et régler leur intensité à chaud, sans redémarrage." },
  { icon: "🗜️", label: "Quantization", desc: "Choisir la précision (Q4/Q5/Q6/Q8) au téléchargement dans le marketplace." },
  { icon: "🧠", label: "Compression du cache KV", desc: "Cache 4/8-bit pour allonger le contexte à VRAM égale." },
  { icon: "🧩", label: "Offload RAM / experts MoE", desc: "Exécuter des modèles plus gros que la VRAM disponible." },
  { icon: "🔮", label: "Décodage spéculatif", desc: "Accélérer la génération avec un petit modèle draft." },
];

const NOT_DOABLE = [
  { label: "Entraînement LoRA / QLoRA", why: "Rétropropagation — nécessite une pile Python (Unsloth, PEFT)." },
  { label: "Distillation", why: "Entraîne un modèle élève depuis un professeur : pile d'entraînement Python." },
  { label: "Abliteration (retrait des refus)", why: "Chirurgie des poids (RepE / transformer-lens), en Python." },
];

/**
 * Model studio: everything the app can really do to a model, stated honestly.
 * llama.cpp is an inference engine — it applies adapters and quantized weights
 * but cannot train. Operations that need a training stack are listed as such,
 * with the practical path (do it in Python, run the result here).
 */
export function ModelStudioView({ onOpenMarketplace, onOpenSettings }: Props) {
  const [caps, setCaps] = useState<RuntimeCapabilities | null>(null);
  const [cfg, setCfg] = useState<InferenceConfig | null>(null);
  const [lora, setLora] = useState<LoraAdapter[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    core.runtimeCapabilities().then((c) => { if (!cancelled) setCaps(c); }).catch(() => {});
    core.getInferenceConfig().then((c) => { if (!cancelled) setCfg(c); }).catch(() => {});
    core.listLoraAdapters().then((l) => { if (!cancelled) setLora(l); }).catch(() => {});
    return () => { cancelled = true; };
  }, []);

  return (
    <section className="lochor-view-container">
      <div className="lochor-view-header">
        <h2>Édition & optimisation de modèle</h2>
        <p className="lochor-view-desc">
          Ce que Lochor peut appliquer à un modèle en local, et ce qui demande une pile
          d'entraînement séparée. Rien n'est simulé ici.
        </p>
      </div>

      <div className="lochor-card">
        <h3>Disponible dans l'application</h3>
        <div className="lochor-caps-grid" style={{ marginTop: 12 }}>
          {DOABLE.map((d) => (
            <div key={d.label} className="lochor-cap-chip">
              <span style={{ flex: "0 0 auto" }}>{d.icon}</span>
              <div style={{ minWidth: 0 }}>
                <div style={{ fontWeight: 600 }}>{d.label}</div>
                <div className="lochor-field-hint" style={{ margin: 0 }}>{d.desc}</div>
              </div>
            </div>
          ))}
        </div>

        <div className="lochor-field" style={{ marginTop: 20 }}>
          <label className="lochor-field-label">Adaptateurs LoRA chargés</label>
          {cfg && cfg.lora_adapters.length > 0 ? (
            <ul className="lochor-lora-list">
              {cfg.lora_adapters.map((p) => (
                <li key={p} className="lochor-lora-row">
                  <span className="lochor-lora-path" title={p}>{p}</span>
                  <span className="lochor-lora-scale">
                    {lora?.find((a) => a.path === p)?.scale?.toFixed(2) ?? "—"}
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="lochor-field-hint" style={{ fontStyle: "italic" }}>
              Aucun adaptateur. Ajoutez-en depuis Paramètres → Moteur IA.
            </p>
          )}
          <div className="lochor-field-actions" style={{ marginTop: 10 }}>
            {onOpenSettings && (
              <button type="button" className="lochor-btn-ghost" onClick={onOpenSettings}>
                ⚙ Gérer les adaptateurs
              </button>
            )}
          </div>
        </div>
      </div>

      <div className="lochor-card" style={{ marginTop: 16 }}>
        <h3>Hors de portée de ce moteur</h3>
        <p className="lochor-field-hint">
          llama.cpp exécute des modèles, il ne les entraîne pas. Ces opérations demandent
          PyTorch et un GPU adapté ; le résultat (GGUF) se charge ensuite ici.
        </p>
        <ul className="lochor-caps-unavailable" style={{ marginTop: 10 }}>
          {NOT_DOABLE.map((n) => (
            <li key={n.label}>
              <strong>{n.label}</strong> — {n.why}
            </li>
          ))}
        </ul>
        <p className="lochor-field-hint" style={{ marginTop: 12 }}>
          En pratique : des modèles <em>déjà</em> distillés ou débridés existent publiquement et
          s'exécutent tels quels dans Lochor.
        </p>
        {onOpenMarketplace && (
          <div className="lochor-field-actions" style={{ marginTop: 10 }}>
            <button type="button" className="lochor-btn-primary" onClick={onOpenMarketplace}>
              🛒 Parcourir le marketplace
            </button>
          </div>
        )}
      </div>

      {caps && (
        <p className="lochor-field-hint" style={{ marginTop: 14 }}>
          Runtime : {caps.runtime_installed ? `llama.cpp ${caps.runtime_version ?? ""}` : "non installé"} ·
          formats supportés : {caps.weight_formats.join(", ")}
        </p>
      )}
    </section>
  );
}
