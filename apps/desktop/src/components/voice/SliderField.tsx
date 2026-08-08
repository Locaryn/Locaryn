/** Tiny labelled slider for voice parameters. Shared across all voice tabs. */

export interface SliderFieldProps {
  label: string;
  min: number;
  max: number;
  step: number;
  value: number;
  disabled: boolean;
  onChange: (v: number) => void;
}

export function SliderField({
  label,
  min,
  max,
  step,
  value,
  disabled,
  onChange,
}: SliderFieldProps) {
  return (
    <div>
      <label
        style={{ fontSize: 10, color: "var(--text-faint)", display: "block", marginBottom: 2 }}
      >
        {label}
      </label>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        disabled={disabled}
        style={{ width: "100%" }}
      />
    </div>
  );
}
