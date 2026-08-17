import { Icon, type IconName } from "@locaryn/ui-core";
import type React from "react";

export const SPEAKER_OPTIONS = [
  { id: "default", label: "Voix par défaut" },
  { id: "serena", label: "Serena" },
  { id: "vivian", label: "Vivian" },
  { id: "ryan", label: "Ryan" },
  { id: "aiden", label: "Aiden" },
  { id: "ono_anna", label: "Ono Anna" },
  { id: "sohee", label: "Sohee" },
  { id: "uncle_fu", label: "Uncle Fu" },
  { id: "eric", label: "Eric (四川方言)" },
  { id: "dylan", label: "Dylan (北京方言)" },
];

export const VOICE_STYLES: {
  id: string;
  label: string;
  icon: IconName;
  desc: string;
  prompt: string;
}[] = [
  {
    id: "narrator",
    label: "Narrateur",
    icon: "models",
    desc: "Clair, posé, documentaire",
    prompt: "Voix de narrateur : claire, posée, ton documentaire",
  },
  {
    id: "conversational",
    label: "Conversationnel",
    icon: "chat",
    desc: "Naturel, chaleureux, quotidien",
    prompt: "Voix conversationnelle : naturelle, chaleureuse, ton quotidien",
  },
  {
    id: "newscaster",
    label: "Journaliste",
    icon: "chart",
    desc: "Professionnel, articulé, neutre",
    prompt: "Voix de journaliste : professionnelle, articulée, ton neutre",
  },
  {
    id: "character",
    label: "Personnage",
    icon: "figures",
    desc: "Expressif, théâtral, varié",
    prompt: "Voix de personnage : expressive, théâtrale, ton varié",
  },
  {
    id: "whisper",
    label: "Chuchotement",
    icon: "private",
    desc: "Doux, intime, proche",
    prompt: "Chuchotement : doux, intime, très proche du micro",
  },
  {
    id: "energetic",
    label: "Énergique",
    icon: "speed",
    desc: "Rapide, enthousiaste, dynamique",
    prompt: "Voix énergique : rapide, enthousiaste, ton dynamique",
  },
];

export interface VoiceCustomTabProps {
  speaker: string;
  styleInstruction: string;
  jobRunning: boolean;
  onSpeakerChange: (v: string) => void;
  onStyleInstructionChange: (v: string) => void;
}

export function VoiceCustomTab({
  speaker,
  styleInstruction,
  jobRunning,
  onSpeakerChange,
  onStyleInstructionChange,
}: VoiceCustomTabProps) {
  const containerStyle: React.CSSProperties = {
    display: "flex",
    flexDirection: "column",
    gap: "20px",
    width: "100%",
  };

  const fieldStyle: React.CSSProperties = {
    display: "flex",
    flexDirection: "column",
    gap: "8px",
  };

  const labelStyle: React.CSSProperties = {
    fontSize: "13px",
    fontWeight: 600,
    color: "var(--text)",
  };

  const inputStyle: React.CSSProperties = {
    backgroundColor: "var(--bg)",
    border: "1px solid var(--border)",
    color: "var(--text)",
    padding: "8px 12px",
    borderRadius: "6px",
    fontSize: "14px",
    outline: "none",
    width: "100%",
    fontFamily: "inherit",
  };

  const textareaStyle: React.CSSProperties = {
    ...inputStyle,
    minHeight: "80px",
    resize: "vertical",
  };

  const gridStyle: React.CSSProperties = {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fill, minmax(130px, 1fr))",
    gap: "10px",
    marginTop: "4px",
  };

  const getPresetButtonStyle = (isActive: boolean): React.CSSProperties => ({
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    justifyContent: "center",
    padding: "12px 8px",
    backgroundColor: "var(--bg)",
    border: `1px solid ${isActive ? "var(--accent)" : "var(--border)"}`,
    borderRadius: "6px",
    cursor: jobRunning ? "not-allowed" : "pointer",
    opacity: jobRunning ? 0.6 : 1,
    transition: "all 0.2s",
  });

  return (
    <div style={containerStyle}>
      <div style={fieldStyle}>
        <label style={labelStyle} htmlFor="speaker-select">
          Locuteur
        </label>
        <select
          id="speaker-select"
          style={inputStyle}
          value={speaker}
          onChange={(e) => onSpeakerChange(e.target.value)}
          disabled={jobRunning}
        >
          {SPEAKER_OPTIONS.map((opt) => (
            <option key={opt.id} value={opt.id}>
              {opt.label}
            </option>
          ))}
        </select>
      </div>

      <div style={fieldStyle}>
        <label style={labelStyle} htmlFor="style-instruction">
          Instruction de style (optionnel)
        </label>
        <textarea
          id="style-instruction"
          style={textareaStyle}
          value={styleInstruction}
          onChange={(e) => onStyleInstructionChange(e.target.value)}
          disabled={jobRunning}
          placeholder="Ex : Parle sur un ton joyeux et énergétique"
        />

        <div style={gridStyle}>
          {VOICE_STYLES.map((style) => {
            const isActive = styleInstruction === style.prompt;
            return (
              <button
                key={style.id}
                type="button"
                style={getPresetButtonStyle(isActive)}
                onClick={() => {
                  if (!jobRunning) {
                    onStyleInstructionChange(style.prompt);
                  }
                }}
                disabled={jobRunning}
              >
                <span style={{ marginBottom: "8px", display: "inline-flex" }}>
                  <Icon name={style.icon} size={22} />
                </span>
                <span style={{ fontSize: "13px", fontWeight: 600, color: "var(--text)" }}>
                  {style.label}
                </span>
                <span
                  style={{
                    fontSize: "11px",
                    color: "var(--text-faint)",
                    textAlign: "center",
                    marginTop: "4px",
                    lineHeight: 1.2,
                  }}
                >
                  {style.desc}
                </span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
