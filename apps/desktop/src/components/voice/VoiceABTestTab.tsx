import { SliderField } from "./SliderField";

export interface ABVoiceParams {
  speaker: string;
  speed: number;
  pitch: number;
  energy: number;
  clarity: number;
}

export interface VoiceABTestTabProps {
  variantA: ABVoiceParams;
  variantB: ABVoiceParams;
  resultA: { label: string } | null;
  resultB: { label: string } | null;
  jobRunning: boolean;
  onChangeA: (p: ABVoiceParams) => void;
  onChangeB: (p: ABVoiceParams) => void;
  onSwap: () => void;
}

const SPEAKER_OPTIONS = [
  { id: "default", label: "Voix par défaut" },
  { id: "male", label: "Homme" },
  { id: "female", label: "Femme" },
  { id: "neutral", label: "Neutre" },
];

function diffLabel(base: ABVoiceParams, other: ABVoiceParams): string {
  const parts: string[] = [];
  if (base.speed !== other.speed) parts.push(`Vitesse ${base.speed.toFixed(1)}x`);
  if (base.pitch !== other.pitch) parts.push(`Hauteur ${base.pitch.toFixed(1)}`);
  if (base.energy !== other.energy) parts.push(`Énergie ${Math.round(base.energy * 100)}%`);
  if (base.clarity !== other.clarity) parts.push(`Clarté ${Math.round(base.clarity * 100)}%`);
  if (base.speaker !== other.speaker) parts.push(`Speaker ${base.speaker}`);
  return parts.join(" · ") || "Identique à l’autre variante";
}

function ParameterColumn({
  title,
  color,
  params,
  onChange,
  result,
  otherParams,
  jobRunning,
}: {
  title: string;
  color: string;
  params: ABVoiceParams;
  onChange: (p: ABVoiceParams) => void;
  result: { label: string } | null;
  otherParams: ABVoiceParams;
  jobRunning: boolean;
}) {
  function update<K extends keyof ABVoiceParams>(key: K, value: ABVoiceParams[K]) {
    onChange({ ...params, [key]: value });
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 4 }}>
        <span
          style={{
            width: 10,
            height: 10,
            borderRadius: "50%",
            background: color,
            flexShrink: 0,
          }}
        />
        <span style={{ fontWeight: 600, fontSize: 13, color: "var(--text)" }}>{title}</span>
      </div>

      <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
        {SPEAKER_OPTIONS.map((s) => {
          const active = params.speaker === s.id;
          return (
            <button
              key={s.id}
              type="button"
              className={`lochor-chip${active ? " lochor-chip-on" : ""}`}
              onClick={() => update("speaker", s.id)}
              disabled={jobRunning}
              style={{ fontSize: 10, padding: "2px 8px" }}
            >
              {s.label}
            </button>
          );
        })}
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr", gap: 8 }}>
        <SliderField
          label={`Vitesse: ${params.speed.toFixed(1)}x`}
          min={0.5}
          max={2.0}
          step={0.1}
          value={params.speed}
          disabled={jobRunning}
          onChange={(v) => update("speed", v)}
        />
        <SliderField
          label={`Hauteur: ${params.pitch.toFixed(1)}`}
          min={0.5}
          max={2.0}
          step={0.1}
          value={params.pitch}
          disabled={jobRunning}
          onChange={(v) => update("pitch", v)}
        />
        <SliderField
          label={`Énergie: ${Math.round(params.energy * 100)}%`}
          min={0}
          max={1}
          step={0.05}
          value={params.energy}
          disabled={jobRunning}
          onChange={(v) => update("energy", v)}
        />
        <SliderField
          label={`Clarté: ${Math.round(params.clarity * 100)}%`}
          min={0}
          max={1}
          step={0.05}
          value={params.clarity}
          disabled={jobRunning}
          onChange={(v) => update("clarity", v)}
        />
      </div>

      {result ? (
        <div style={{ marginTop: "auto", paddingTop: 8 }}>
          <div
            style={{
              fontSize: 10,
              color: "var(--text-faint)",
              marginBottom: 4,
              display: "flex",
              alignItems: "center",
              gap: 6,
            }}
          >
            <span
              style={{
                width: 8,
                height: 8,
                borderRadius: "50%",
                background: color,
              }}
            />
            {result.label}
          </div>
          <div style={{ fontSize: 10, color: "var(--text-faint)", lineHeight: 1.4 }}>
            {diffLabel(params, otherParams)}
          </div>
        </div>
      ) : (
        <div style={{ marginTop: "auto", paddingTop: 8, fontSize: 11, color: "var(--text-faint)" }}>
          Non généré
        </div>
      )}
    </div>
  );
}

export function VoiceABTestTab({
  variantA,
  variantB,
  resultA,
  resultB,
  jobRunning,
  onChangeA,
  onChangeB,
  onSwap,
}: VoiceABTestTabProps) {
  return (
    <div>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          marginBottom: 12,
        }}
      >
        <p style={{ margin: 0, fontSize: 12, color: "var(--text-faint)" }}>
          Comparez deux variantes du même texte avec des paramètres différents.
        </p>
        <button
          type="button"
          className="lochor-btn-ghost"
          onClick={onSwap}
          disabled={jobRunning}
          style={{ fontSize: 11, padding: "4px 10px" }}
        >
          Échanger A ↔ B
        </button>
      </div>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr 1fr",
          gap: 16,
        }}
      >
        <div
          style={{
            padding: 12,
            borderRadius: 8,
            border: "1px solid var(--border)",
            background: "rgba(100, 150, 255, 0.04)",
          }}
        >
          <ParameterColumn
            title="Variante A"
            color="#6496ff"
            params={variantA}
            onChange={onChangeA}
            result={resultA}
            otherParams={variantB}
            jobRunning={jobRunning}
          />
        </div>
        <div
          style={{
            padding: 12,
            borderRadius: 8,
            border: "1px solid var(--border)",
            background: "rgba(255, 150, 100, 0.04)",
          }}
        >
          <ParameterColumn
            title="Variante B"
            color="#ff9656"
            params={variantB}
            onChange={onChangeB}
            result={resultB}
            otherParams={variantA}
            jobRunning={jobRunning}
          />
        </div>
      </div>
    </div>
  );
}
