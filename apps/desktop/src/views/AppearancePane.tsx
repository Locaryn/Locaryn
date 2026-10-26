import { Icon, type IconName, type ThemeMode } from "@locaryn/ui-core";
import { ACCENT_PRESETS, type UseThemeReturn } from "../hooks/useTheme";

/** Les trois réglages de thème, dans l'ordre où ils se lisent. */
const THEME_MODES: { value: ThemeMode; label: string; icon: IconName }[] = [
  { value: "dark", label: "Sombre", icon: "moon" },
  { value: "light", label: "Clair", icon: "sun" },
  { value: "system", label: "Système", icon: "monitor" },
];

/**
 * Thème et couleur d'accentuation.
 *
 * Vivait dans la section « Apparence » des réglages ; rejoint le compte, où
 * l'utilisateur règle déjà son identité : l'apparence est une préférence de
 * profil, pas un réglage d'infrastructure. Rendue par le compte et réutilisable
 * partout où ce réglage a du sens.
 */
export function AppearancePane({ theme }: { theme: UseThemeReturn }) {
  const { settings, updateAccent, updateMode, resetTheme } = theme;
  return (
    <div className="locaryn-field">
      <div className="locaryn-field-label">Thème</div>
      <p className="locaryn-field-hint">
        Sombre par défaut. En clair, l'accent s'assombrit tout seul pour rester lisible.
      </p>
      <div className="locaryn-segmented" style={{ marginTop: 12 }} role="group">
        {THEME_MODES.map((m) => (
          <button
            key={m.value}
            type="button"
            className={`locaryn-segment${settings.mode === m.value ? " locaryn-segment-on" : ""}`}
            aria-pressed={settings.mode === m.value}
            onClick={() => updateMode(m.value)}
          >
            <Icon name={m.icon} size={14} />
            {m.label}
          </button>
        ))}
      </div>

      <div className="locaryn-field-label" style={{ marginTop: 32 }}>
        Couleur d'accentuation
      </div>
      <p className="locaryn-field-hint">
        La teinte unique de l'interface. Sobre et naturelle par défaut.
      </p>
      <div className="locaryn-swatch-grid" style={{ marginTop: 12 }}>
        {ACCENT_PRESETS.map((p) => (
          <button
            key={p.hex}
            type="button"
            className={`locaryn-swatch${settings.accentHex === p.hex ? " locaryn-swatch-active" : ""}`}
            style={{ background: p.hex }}
            title={p.name}
            aria-label={`Accent ${p.name}`}
            onClick={() => updateAccent(p.hex)}
          >
            {settings.accentHex === p.hex && (
              <span className="locaryn-swatch-check">
                <Icon name="check" size={14} />
              </span>
            )}
          </button>
        ))}
      </div>
      <div className="locaryn-custom-color" style={{ marginTop: 16 }}>
        <input
          type="color"
          value={settings.accentHex}
          onChange={(e) => updateAccent(e.target.value)}
          className="locaryn-color-input"
          aria-label="Couleur personnalisée"
        />
        <span className="locaryn-color-value">{settings.accentHex}</span>
      </div>
      <button
        type="button"
        className="locaryn-settings-reset"
        style={{ marginTop: 16 }}
        onClick={resetTheme}
      >
        Réinitialiser l'apparence
      </button>
    </div>
  );
}
