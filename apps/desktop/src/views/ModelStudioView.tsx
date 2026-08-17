import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import {
  type InferenceConfig,
  type LoraAdapter,
  type RuntimeCapabilities,
  core,
} from "../lib/core";

type Props = {
  onOpenMarketplace?: () => void;
  onOpenSettings?: () => void;
};

/** What can genuinely be done to a model from this app, and what cannot. */
const DOABLE: { icon: IconName; label: string; desc: string }[] = [
  {
    icon: "target",
    label: "Adaptateurs LoRA",
    desc: "Charger des .gguf et régler leur intensité à chaud, sans redémarrage.",
  },
  {
    icon: "archive",
    label: "Quantization",
    desc: "Choisir la précision (Q4/Q5/Q6/Q8) au téléchargement dans le marketplace.",
  },
  {
    icon: "memory",
    label: "Compression du cache KV",
    desc: "Cache 4/8-bit pour allonger le contexte à VRAM égale.",
  },
  {
    icon: "extensions",
    label: "Offload RAM / experts MoE",
    desc: "Exécuter des modèles plus gros que la VRAM disponible.",
  },
  {
    icon: "star",
    label: "Décodage spéculatif",
    desc: "Accélérer la génération avec un petit modèle draft.",
  },
];

const NOT_DOABLE = [
  {
    label: "Entraînement LoRA / QLoRA",
    why: "Rétropropagation — nécessite une pile Python (Unsloth, PEFT).",
  },
  {
    label: "Distillation",
    why: "Entraîne un modèle élève depuis un professeur : pile d'entraînement Python.",
  },
  {
    label: "Abliteration (retrait des refus)",
    why: "Chirurgie des poids (RepE / transformer-lens), en Python.",
  },
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
    core
      .runtimeCapabilities()
      .then((c) => {
        if (!cancelled) setCaps(c);
      })
      .catch(() => {});
    core
      .getInferenceConfig()
      .then((c) => {
        if (!cancelled) setCfg(c);
      })
      .catch(() => {});
    core
      .listLoraAdapters()
      .then((l) => {
        if (!cancelled) setLora(l);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <section className="locaryn-view-container">
      <div className="locaryn-view-header">
        <h2>Édition & optimisation de modèle</h2>
        <p className="locaryn-view-desc">
          Ce que Locaryn peut appliquer à un modèle en local, et ce qui demande une pile
          d'entraînement séparée. Rien n'est simulé ici.
        </p>
      </div>

      <div className="locaryn-card">
        <h3>Disponible dans l'application</h3>
        <div className="locaryn-caps-grid" style={{ marginTop: 12 }}>
          {DOABLE.map((d) => (
            <div key={d.label} className="locaryn-cap-chip">
              <span style={{ flex: "0 0 auto", display: "inline-flex" }}>
                <Icon name={d.icon} size={18} />
              </span>
              <div style={{ minWidth: 0 }}>
                <div style={{ fontWeight: 600 }}>{d.label}</div>
                <div className="locaryn-field-hint" style={{ margin: 0 }}>
                  {d.desc}
                </div>
              </div>
            </div>
          ))}
        </div>

        <div className="locaryn-field" style={{ marginTop: 20 }}>
          <div className="locaryn-field-label">Adaptateurs LoRA chargés</div>
          {cfg && cfg.lora_adapters.length > 0 ? (
            <ul className="locaryn-lora-list">
              {cfg.lora_adapters.map((p) => (
                <li key={p} className="locaryn-lora-row">
                  <span className="locaryn-lora-path" title={p}>
                    {p}
                  </span>
                  <span className="locaryn-lora-scale">
                    {lora?.find((a) => a.path === p)?.scale?.toFixed(2) ?? "—"}
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="locaryn-field-hint" style={{ fontStyle: "italic" }}>
              Aucun adaptateur. Ajoutez-en depuis Paramètres → Moteur IA.
            </p>
          )}
          <div className="locaryn-field-actions" style={{ marginTop: 10 }}>
            {onOpenSettings && (
              <button type="button" className="locaryn-btn-ghost" onClick={onOpenSettings}>
                <Icon name="settings" size={15} /> Gérer les adaptateurs
              </button>
            )}
          </div>
        </div>
      </div>

      <div className="locaryn-card" style={{ marginTop: 16 }}>
        <h3>Hors de portée de ce moteur</h3>
        <p className="locaryn-field-hint">
          llama.cpp exécute des modèles, il ne les entraîne pas. Ces opérations demandent PyTorch et
          un GPU adapté ; le résultat (GGUF) se charge ensuite ici.
        </p>
        <ul className="locaryn-caps-unavailable" style={{ marginTop: 10 }}>
          {NOT_DOABLE.map((n) => (
            <li key={n.label}>
              <strong>{n.label}</strong> — {n.why}
            </li>
          ))}
        </ul>
        <p className="locaryn-field-hint" style={{ marginTop: 12 }}>
          En pratique : des modèles <em>déjà</em> distillés ou débridés existent publiquement et
          s'exécutent tels quels dans Locaryn.
        </p>
        {onOpenMarketplace && (
          <div className="locaryn-field-actions" style={{ marginTop: 10 }}>
            <button type="button" className="locaryn-btn-primary" onClick={onOpenMarketplace}>
              <Icon name="marketplace" size={15} /> Parcourir le marketplace
            </button>
          </div>
        )}
      </div>

      {caps && (
        <p className="locaryn-field-hint" style={{ marginTop: 14 }}>
          Runtime :{" "}
          {caps.runtime_installed ? `llama.cpp ${caps.runtime_version ?? ""}` : "non installé"} ·
          formats supportés : {caps.weight_formats.join(", ")}
        </p>
      )}
    </section>
  );
}
