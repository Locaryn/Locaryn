import React from 'react';

export interface VoiceDesignTabProps {
  designPrompt: string;
  jobRunning: boolean;
  onPromptChange: (v: string) => void;
}

const INSPIRATIONS = [
  "Voix chaleureuse de grand-mère qui raconte une histoire",
  "Présentateur radio, voix grave et posée, années 50",
  "Jeune femme enthousiaste, ton montant en fin de phrase",
  "Robot poli et légèrement sarcastique",
  "Chuchotement ASMR, très proche du micro",
  "Voix d'elfe, cristalline, réverbération naturelle",
];

export const VoiceDesignTab: React.FC<VoiceDesignTabProps> = ({
  designPrompt,
  jobRunning,
  onPromptChange,
}) => {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '24px', padding: '16px' }}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
        <label
          style={{
            fontSize: '14px',
            fontWeight: 600,
            color: 'var(--text)'
          }}
        >
          Description de la voix
        </label>
        <textarea
          value={designPrompt}
          onChange={(e) => onPromptChange(e.target.value)}
          disabled={jobRunning}
          placeholder="Speak in an incredulous tone, but with a hint of panic beginning to creep into your voice."
          style={{
            width: '100%',
            minHeight: '120px',
            resize: 'vertical',
            padding: '12px',
            backgroundColor: 'var(--bg)',
            color: 'var(--text)',
            border: '1px solid var(--border)',
            borderRadius: '6px',
            fontSize: '14px',
            lineHeight: '1.5',
            fontFamily: 'inherit',
            outline: 'none',
            boxSizing: 'border-box'
          }}
        />
        <p style={{ margin: 0, fontSize: '12px', color: 'var(--text-faint)' }}>
          Décrivez le ton, l'émotion, l'accent et les caractéristiques de la voix.
        </p>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
        <span style={{ fontSize: '14px', fontWeight: 600, color: 'var(--text)' }}>
          Inspiration
        </span>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
          {INSPIRATIONS.map((insp, idx) => (
            <button
              key={idx}
              disabled={jobRunning}
              onClick={() => onPromptChange(insp)}
              style={{
                background: 'transparent',
                border: '1px solid var(--border)',
                borderRadius: '16px',
                padding: '6px 12px',
                fontSize: '13px',
                color: 'var(--text-faint)',
                cursor: jobRunning ? 'not-allowed' : 'pointer',
                transition: 'all 0.2s',
                opacity: jobRunning ? 0.5 : 1
              }}
              onMouseEnter={(e) => {
                if (!jobRunning) {
                  e.currentTarget.style.color = 'var(--text)';
                  e.currentTarget.style.borderColor = 'var(--accent)';
                }
              }}
              onMouseLeave={(e) => {
                if (!jobRunning) {
                  e.currentTarget.style.color = 'var(--text-faint)';
                  e.currentTarget.style.borderColor = 'var(--border)';
                }
              }}
            >
              {insp}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
};
