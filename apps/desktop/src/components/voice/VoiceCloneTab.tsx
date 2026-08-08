import type React from "react";
import { useState } from "react";
import { VOICE_SETTINGS_DEFAULTS, type VoicePreset } from "../../lib/core";
import { SliderField } from "./SliderField";
import { VoicePresetBar } from "./VoicePresetBar";

export interface VoiceCloneTabProps {
  voiceFile: string | null;
  voiceFileUrl: string | null;
  voiceName: string;
  referenceText: string;
  instruct: string;
  isRecording: boolean;
  recordTime: number;
  speed: number;
  pitch: number;
  /** Sampling temperature. Higher = more varied intonation. */
  temperature: number;
  /** In-context cloning: carry the reference speaker's rhythm, not just timbre. */
  expressive: boolean;
  energy: number;
  clarity: number;
  jobRunning: boolean;
  onPickVoice: () => void;
  onStartRecording: () => void;
  onStopRecording: () => void;
  onClearVoice: () => void;
  onReferenceTextChange: (v: string) => void;
  onInstructChange: (v: string) => void;
  onSpeedChange: (v: number) => void;
  onTemperatureChange: (v: number) => void;
  onExpressiveChange: (v: boolean) => void;
  /** Currently selected TTS model, to report what it honours. */
  model: string;
  language: string;
  pauseScale: number;
  onPauseScaleChange: (v: number) => void;
  /** Restore a saved voice: recording, transcript and every setting. */
  onApplyPreset: (p: VoicePreset) => void;
  onPitchChange: (v: number) => void;
  onEnergyChange: (v: number) => void;
  onClarityChange: (v: number) => void;
}

export function VoiceCloneTab({
  voiceFile,
  voiceFileUrl,
  voiceName,
  referenceText,
  instruct,
  isRecording,
  recordTime,
  speed,
  pitch,
  temperature,
  expressive,
  energy,
  clarity,
  jobRunning,
  onPickVoice,
  onStartRecording,
  onStopRecording,
  onClearVoice,
  onReferenceTextChange,
  onInstructChange,
  onSpeedChange,
  onTemperatureChange,
  onExpressiveChange,
  model,
  language,
  pauseScale,
  onPauseScaleChange,
  onApplyPreset,
  onPitchChange,
  onEnergyChange,
  onClarityChange,
}: VoiceCloneTabProps) {
  const [instructOpen, setInstructOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

  const formatTime = (secs: number) => {
    const m = Math.floor(secs / 60)
      .toString()
      .padStart(2, "0");
    const s = (secs % 60).toString().padStart(2, "0");
    return `${m}:${s}`;
  };

  const containerStyle: React.CSSProperties = {
    display: "flex",
    flexDirection: "column",
    gap: "16px",
  };

  const sectionStyle: React.CSSProperties = {
    display: "flex",
    flexDirection: "column",
    gap: "8px",
    backgroundColor: "var(--bg-secondary, #1e1e1e)",
    padding: "16px",
    borderRadius: "8px",
    border: "1px solid var(--border-color, #333)",
  };

  const headerStyle: React.CSSProperties = {
    fontSize: "14px",
    fontWeight: "600",
    color: "var(--text-primary, #fff)",
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    cursor: "pointer",
    userSelect: "none",
  };

  const textStyle: React.CSSProperties = {
    fontSize: "13px",
    color: "var(--text-secondary, #aaa)",
    lineHeight: "1.4",
  };

  const buttonRowStyle: React.CSSProperties = {
    display: "flex",
    gap: "8px",
    alignItems: "center",
  };

  const buttonStyle: React.CSSProperties = {
    padding: "8px 16px",
    borderRadius: "6px",
    border: "1px solid var(--border-color, #444)",
    backgroundColor: "var(--bg-tertiary, #2d2d2d)",
    color: "var(--text-primary, #fff)",
    cursor: "pointer",
    fontSize: "13px",
    display: "flex",
    alignItems: "center",
    gap: "6px",
    transition: "background-color 0.2s",
  };

  const removeButtonStyle: React.CSSProperties = {
    ...buttonStyle,
    color: "#ff6b6b",
    borderColor: "#4d2a2a",
    backgroundColor: "#3a2020",
  };

  const recordingButtonStyle: React.CSSProperties = {
    ...buttonStyle,
    color: "#ff6b6b",
    borderColor: "#ff6b6b",
  };

  const textareaStyle: React.CSSProperties = {
    width: "100%",
    minHeight: "80px",
    backgroundColor: "var(--bg-tertiary, #2d2d2d)",
    border: "1px solid var(--border-color, #444)",
    borderRadius: "6px",
    padding: "12px",
    color: "var(--text-primary, #fff)",
    fontSize: "13px",
    resize: "vertical",
    outline: "none",
    fontFamily: "inherit",
    boxSizing: "border-box",
  };

  const fileInfoStyle: React.CSSProperties = {
    display: "flex",
    alignItems: "center",
    gap: "12px",
    marginTop: "8px",
    backgroundColor: "var(--bg-tertiary, #2a2a2a)",
    padding: "12px",
    borderRadius: "6px",
    border: "1px solid var(--border-color, #333)",
  };

  return (
    <div style={containerStyle}>
      <VoicePresetBar
        model={model}
        referenceAudio={voiceFile}
        referenceText={referenceText}
        language={language}
        settings={{
          ...VOICE_SETTINGS_DEFAULTS,
          speed,
          pitch,
          energy,
          clarity,
          pauseScale,
          temperature,
          expressive,
          instruct,
        }}
        jobRunning={jobRunning}
        onApply={onApplyPreset}
      />

      <p style={textStyle}>
        Fournissez un échantillon audio de 3 à 10 secondes pour cloner une voix, ou importez une
        vidéo (MP4, MKV, AVI…) dont l'audio sera extrait automatiquement.
      </p>

      {/* Reference Audio */}
      <div style={sectionStyle}>
        <div style={{ ...headerStyle, cursor: "default" }}>Audio de référence</div>
        <div style={buttonRowStyle}>
          <button style={buttonStyle} onClick={onPickVoice} disabled={jobRunning || isRecording}>
            📁 Importer un fichier
          </button>
          {!isRecording ? (
            <button style={buttonStyle} onClick={onStartRecording} disabled={jobRunning}>
              🎤 Enregistrer
            </button>
          ) : (
            <button style={recordingButtonStyle} onClick={onStopRecording}>
              ⏹ Arrêter l'enregistrement ({formatTime(recordTime)})
            </button>
          )}
        </div>

        {voiceFile && (
          <div style={fileInfoStyle}>
            <span style={{ fontSize: "16px" }}>✅</span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div
                style={{
                  fontSize: "13px",
                  fontWeight: "500",
                  color: "#fff",
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                {voiceName || "Audio sélectionné"}
              </div>
              {voiceFileUrl && (
                <audio
                  src={voiceFileUrl}
                  controls
                  style={{ width: "100%", height: "32px", marginTop: "8px" }}
                />
              )}
            </div>
            <button style={removeButtonStyle} onClick={onClearVoice} disabled={jobRunning}>
              Retirer
            </button>
          </div>
        )}
      </div>

      {/* Reference Text */}
      <div style={sectionStyle}>
        <div style={{ ...headerStyle, cursor: "default" }}>Texte de référence (optionnel)</div>
        <textarea
          style={textareaStyle}
          value={referenceText}
          onChange={(e) => onReferenceTextChange(e.target.value)}
          placeholder="Transcription de l'audio de référence. Laisser vide pour transcrire automatiquement."
          disabled={jobRunning}
        />
      </div>

      {/* Instruct */}
      <div style={sectionStyle}>
        <div style={headerStyle} onClick={() => setInstructOpen(!instructOpen)}>
          <span>Instruction (optionnel)</span>
          <span>{instructOpen ? "▲" : "▼"}</span>
        </div>
        {instructOpen && (
          <div style={{ marginTop: "12px" }}>
            <textarea
              style={textareaStyle}
              value={instruct}
              onChange={(e) => onInstructChange(e.target.value)}
              placeholder="Instructions pour la génération de voix (ex: Parler doucement et calmement)"
              disabled={jobRunning}
            />
          </div>
        )}
      </div>

      {/* Generation Settings */}
      <div style={sectionStyle}>
        <div style={headerStyle} onClick={() => setSettingsOpen(!settingsOpen)}>
          <span>Paramètres de génération (optionnel)</span>
          <span>{settingsOpen ? "▲" : "▼"}</span>
        </div>
        {settingsOpen && (
          <div style={{ display: "flex", flexDirection: "column", gap: "16px", marginTop: "16px" }}>
            <label
              style={{
                display: "flex",
                alignItems: "flex-start",
                gap: 10,
                cursor: jobRunning ? "default" : "pointer",
              }}
            >
              <input
                type="checkbox"
                checked={expressive}
                disabled={jobRunning}
                onChange={(e) => onExpressiveChange(e.target.checked)}
                style={{ marginTop: 3 }}
              />
              <span>
                <span style={{ display: "block", fontSize: 13, color: "var(--text)" }}>
                  Reprendre l'intonation de la référence
                </span>
                <span
                  style={{
                    display: "block",
                    fontSize: 11,
                    color: "var(--text-faint)",
                    marginTop: 2,
                  }}
                >
                  Le modèle s'appuie sur l'audio de référence <em>et</em> sa transcription : il
                  reprend le rythme et les inflexions, pas seulement le timbre. Décoché, la phrase
                  est lue platement.
                </span>
              </span>
            </label>
            <SliderField
              label="Expressivité (température)"
              value={temperature}
              min={0.3}
              max={1.4}
              step={0.05}
              onChange={onTemperatureChange}
              disabled={jobRunning}
            />
            <p style={{ fontSize: 11, color: "var(--text-faint)", lineHeight: 1.5, margin: 0 }}>
              Les quatre réglages ci-dessous s'appliquent au signal rendu, pas à la façon dont le
              modèle choisit de parler — ils fonctionnent donc sur tous les moteurs, y compris ceux
              qui n'exposent aucun de ces contrôles.
            </p>
            <SliderField
              label="Longueur des pauses"
              value={pauseScale}
              min={0.4}
              max={2.5}
              step={0.05}
              onChange={onPauseScaleChange}
              disabled={jobRunning}
            />
            <SliderField
              label="Vitesse"
              value={speed}
              min={0.5}
              max={1.5}
              step={0.1}
              onChange={onSpeedChange}
              disabled={jobRunning}
            />
            <SliderField
              label="Hauteur de voix"
              value={pitch}
              min={0.5}
              max={2.0}
              step={0.1}
              onChange={onPitchChange}
              disabled={jobRunning}
            />
            <SliderField
              label="Présence (énergie)"
              value={energy}
              min={0}
              max={1}
              step={0.05}
              onChange={onEnergyChange}
              disabled={jobRunning}
            />
            <SliderField
              label="Clarté des consonnes"
              value={clarity}
              min={0}
              max={1}
              step={0.05}
              onChange={onClarityChange}
              disabled={jobRunning}
            />
          </div>
        )}
      </div>
    </div>
  );
}
