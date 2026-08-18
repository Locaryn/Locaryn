import { useEffect, useState } from "react";
import { type ModelPreferences, core } from "../lib/core";

/** Models that can actually synthesize speech; tokenizer/config files are not choices. */
function isTtsModel(model: string): boolean {
  const lower = model.toLowerCase();
  if (/tokenizer|config\.json|vocab\.json|merges\.txt|preprocessor_config/i.test(lower)) {
    return false;
  }
  return (
    /piper|xtts|coqui|melotts|kokoro|parler|chatterbox|voxcpm2|omnivoice|f5[-_.]?tts|qwen3[-_.]?tts|moss[-_.]?tts|higgs[-_.]?tts|vibevoice|pocket[-_.]?tts/.test(
      lower,
    ) || lower.endsWith(".onnx")
  );
}

/** The model used by the Studio when no voice model was selected in the current task. */
export function TtsModelSetting() {
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
          core.listProviders(),
        ]);
        const active = providers.find((p) => p.is_active) ?? providers[0];
        const installed = active ? await core.listModels(active.endpoint) : [];
        if (cancelled) return;
        setPreferences(saved);
        setModels([...new Set(installed.filter(isTtsModel))]);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  async function choose(ttsModel: string | null) {
    if (!preferences) return;
    const next = { ...preferences, tts_model: ttsModel };
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
      <div className="locaryn-field-label">Modèle TTS par défaut</div>
      <p className="locaryn-field-hint">
        Le modèle utilisé automatiquement par la synthèse vocale quand aucune voix n'est choisie
        dans le Studio. Le choix reste modifiable pour une génération ponctuelle.
      </p>
      <select
        className="locaryn-select"
        value={preferences?.tts_model ?? ""}
        disabled={busy || !preferences}
        onChange={(e) => void choose(e.target.value || null)}
      >
        <option value="">Automatique — premier modèle TTS installé</option>
        {preferences?.tts_model && !models.includes(preferences.tts_model) && (
          <option value={preferences.tts_model}>{preferences.tts_model} — non installé</option>
        )}
        {models.map((model) => (
          <option key={model} value={model}>
            {model}
          </option>
        ))}
      </select>
      {models.length === 0 && (
        <p className="locaryn-field-hint">
          Aucun modèle TTS détecté. Installez-en un depuis le Marketplace pour pouvoir le choisir.
        </p>
      )}
      {error && <div className="locaryn-vp-error">{error}</div>}
    </div>
  );
}
