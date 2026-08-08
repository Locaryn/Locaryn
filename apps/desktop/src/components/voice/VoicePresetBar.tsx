import { useCallback, useEffect, useState } from "react";
import { type EngineSupport, type VoicePreset, type VoiceSettings, core } from "../../lib/core";

type Props = {
  /** Model currently selected, to report what it honours. */
  model: string;
  /** Reference recording on disk, if one is loaded. */
  referenceAudio: string | null;
  referenceText: string;
  language: string;
  settings: VoiceSettings;
  jobRunning: boolean;
  /** Load a preset: restores its recording, transcript and settings. */
  onApply: (preset: VoicePreset) => void;
};

/**
 * Saved voices: pick one, or capture the current setup as a new one.
 *
 * The point is to stop re-uploading the same recording and re-tuning the same
 * sliders for every sentence. A preset carries its own copy of the reference
 * audio, so it keeps working after the original file is moved or deleted.
 */
export function VoicePresetBar({
  model,
  referenceAudio,
  referenceText,
  language,
  settings,
  jobRunning,
  onApply,
}: Props) {
  const [presets, setPresets] = useState<VoicePreset[]>([]);
  const [support, setSupport] = useState<EngineSupport | null>(null);
  const [saving, setSaving] = useState(false);
  const [name, setName] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setPresets(await core.listVoicePresets());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!model) return;
    core
      .voicePresetSupport(model)
      .then(setSupport)
      .catch(() => setSupport(null));
  }, [model]);

  async function handleSave() {
    setError(null);
    setNotice(null);
    if (!name.trim()) {
      setError("Donnez un nom au préréglage.");
      return;
    }
    if (!referenceAudio) {
      setError("Chargez ou enregistrez une voix de référence avant d'enregistrer.");
      return;
    }
    try {
      const saved = await core.saveVoicePreset({
        name: name.trim(),
        note: note.trim(),
        referenceAudio,
        referenceText,
        language,
        engine: support?.engine ?? "",
        settings,
      });
      setNotice(`« ${saved.name} » enregistré.`);
      setName("");
      setNote("");
      setSaving(false);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleDelete(p: VoicePreset) {
    setError(null);
    try {
      await core.deleteVoicePreset(p.id);
      setNotice(`« ${p.name} » supprimé.`);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  // Settings this model will quietly ignore — worth naming rather than letting
  // the user tune a slider that does nothing.
  const ignored: string[] = [];
  if (support) {
    if (!support.referenceText) ignored.push("le texte de référence");
    if (!support.temperature) ignored.push("l'expressivité");
    if (!support.pitch) ignored.push("la hauteur");
    if (!support.instruct) ignored.push("la consigne de style");
  }

  return (
    <div className="locaryn-vp">
      <div className="locaryn-vp-head">
        <span className="locaryn-vp-title">Voix enregistrées</span>
        <button
          type="button"
          className="locaryn-btn-ghost locaryn-vp-add"
          disabled={jobRunning}
          onClick={() => {
            setSaving((s) => !s);
            setError(null);
            setNotice(null);
          }}
        >
          {saving ? "Annuler" : "+ Enregistrer la voix actuelle"}
        </button>
      </div>

      {support && !support.cloning && (
        <p className="locaryn-vp-warn">
          {support.engine} ne sait pas cloner depuis un enregistrement : les préréglages ne
          s'appliqueront que partiellement. Choisissez un modèle de clonage (Qwen3-TTS Base, XTTS ou
          F5-TTS) pour utiliser une voix enregistrée.
        </p>
      )}
      {support && support.cloning && ignored.length > 0 && (
        <p className="locaryn-vp-hint">
          {support.engine} ignore {ignored.join(", ")}. Le reste du préréglage s'applique.
        </p>
      )}

      {saving && (
        <div className="locaryn-vp-form">
          <input
            className="locaryn-input"
            placeholder="Nom — ex. « Ma petite sœur »"
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus
          />
          <input
            className="locaryn-input"
            placeholder="Note (facultatif) — timbre, usage…"
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
          <p className="locaryn-vp-hint">
            L'enregistrement de référence est copié dans le préréglage : il restera utilisable même
            si vous déplacez ou supprimez le fichier d'origine.
          </p>
          <button type="button" className="locaryn-btn-primary" onClick={handleSave}>
            Enregistrer
          </button>
        </div>
      )}

      {presets.length === 0 ? (
        <p className="locaryn-vp-empty">
          Aucune voix enregistrée. Chargez un extrait, réglez la voix, puis enregistrez-la pour la
          réutiliser d'un clic.
        </p>
      ) : (
        <div className="locaryn-vp-list">
          {presets.map((p) => (
            <div className="locaryn-vp-item" key={p.id}>
              <button
                type="button"
                className="locaryn-vp-pick"
                disabled={jobRunning}
                onClick={() => {
                  onApply(p);
                  setNotice(`« ${p.name} » chargé.`);
                }}
                title={p.note || p.referenceText || p.name}
              >
                <span className="locaryn-vp-name">{p.name}</span>
                <span className="locaryn-vp-meta">
                  {p.durationS > 0 ? `${p.durationS.toFixed(0)} s` : "—"}
                  {p.language ? ` · ${p.language}` : ""}
                  {p.settings.expressive ? " · intonation" : " · timbre seul"}
                </span>
                {p.note && <span className="locaryn-vp-note">{p.note}</span>}
              </button>
              <button
                type="button"
                className="locaryn-icon-btn"
                disabled={jobRunning}
                onClick={() => handleDelete(p)}
                aria-label={`Supprimer ${p.name}`}
                title="Supprimer"
              >
                ✕
              </button>
            </div>
          ))}
        </div>
      )}

      {error && <div className="locaryn-vp-error">{error}</div>}
      {notice && !error && <div className="locaryn-vp-notice">{notice}</div>}
    </div>
  );
}
