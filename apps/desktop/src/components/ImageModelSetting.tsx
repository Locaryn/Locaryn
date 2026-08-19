import { useEffect, useState } from "react";
import { type ModelPreferences, core } from "../lib/core";

function isImageModel(model: string): boolean {
  const lower = model.toLowerCase();
  if (/tokenizer|config\.json|vocab\.json|merges\.txt|preprocessor_config/i.test(lower)) {
    return false;
  }
  return /sd|flux|diffusion|stable|z-image|sdxl|dall-e|imagen|qwen3-image/i.test(lower);
}

/** The default diffusion model used for image generation tasks. */
export function ImageModelSetting() {
  const [models, setModels] = useState<string[]>([]);
  const [preferences, setPreferences] = useState<ModelPreferences | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [saved, providers] = await Promise.all([
          core.getModelPreferences(),
          core.listProviders().catch(() => []),
        ]);

        const discoveredModels: string[] = [];

        // 1. Check extension MCP tool list_image_models if available
        try {
          const res = await core.invokeExtensionTool("list_image_models", {});
          if (res) {
            const parsed = typeof res === "string" ? JSON.parse(res) : res;
            const extModels: string[] = Array.isArray(parsed)
              ? parsed
              : Array.isArray(parsed?.models)
                ? parsed.models
                : [];
            discoveredModels.push(...extModels);
          }
        } catch {
          // Extension might not be running yet or no MCP tool
        }

        // 2. Check installed provider models
        const active = providers.find((p) => p.is_active) ?? providers[0];
        if (active) {
          try {
            const providerModels = await core.listModels(active.endpoint);
            discoveredModels.push(...providerModels.filter(isImageModel));
          } catch {
            // Ignore
          }
        }

        if (cancelled) return;
        setPreferences(saved);
        setModels([...new Set(discoveredModels)]);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  async function choose(imageModel: string | null) {
    if (!preferences) return;
    const next = { ...preferences, image_model: imageModel };
    setPreferences(next);
    setBusy(true);
    setError(null);
    try {
      await core.setModelPreferences(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="locaryn-model-preference-field">
      <div className="locaryn-field-label">Modèle d'image par défaut</div>
      <p className="locaryn-field-hint">
        Le modèle utilisé par défaut pour la génération et retouche d'images (texte vers image,
        illustrations du chat, Studio).
      </p>
      <select
        className="locaryn-select"
        value={preferences?.image_model ?? ""}
        disabled={busy || !preferences}
        onChange={(e) => void choose(e.target.value || null)}
      >
        <option value="">Automatique — premier modèle de diffusion installé</option>
        {preferences?.image_model && !models.includes(preferences.image_model) && (
          <option value={preferences.image_model}>{preferences.image_model} — non installé</option>
        )}
        {models.map((model) => (
          <option key={model} value={model}>
            {model}
          </option>
        ))}
      </select>
      {models.length === 0 && (
        <p className="locaryn-field-hint">
          Aucun modèle d'image détecté. Installez un checkpoint de diffusion depuis l'extension
          Génération d'images pour pouvoir le choisir.
        </p>
      )}
      {error && <div className="locaryn-vp-error">{error}</div>}
    </div>
  );
}
