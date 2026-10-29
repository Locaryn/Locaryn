import { Icon } from "@locaryn/ui-core";
import { type UiMode, updateProfile, useProfile } from "../lib/profile";

const MODES: { id: UiMode; icon: "sparkle" | "terminal-window"; label: string; hint: string }[] = [
  {
    id: "simple",
    icon: "sparkle",
    label: "Simplifiée",
    hint: "L'essentiel seulement. Catalogues et réglages techniques rangés dans les paramètres avancés.",
  },
  {
    id: "expert",
    icon: "terminal-window",
    label: "Complète",
    hint: "Toute l'application : modèles, extensions, terminal et réglages à portée de main.",
  },
];

/** Bascule entre l'interface simplifiée et l'interface complète. */
export function InterfaceModeSetting() {
  const { mode } = useProfile();
  return (
    <div className="locaryn-field">
      <div className="locaryn-field-label">Interface</div>
      <p className="locaryn-field-hint">
        Le niveau de détail affiché. Rien n'est supprimé en mode simplifié : tout reste accessible.
      </p>
      <div className="locaryn-mode-grid">
        {MODES.map((m) => (
          <button
            key={m.id}
            type="button"
            className={`locaryn-mode-card${mode === m.id ? " locaryn-active" : ""}`}
            aria-pressed={mode === m.id}
            onClick={() => updateProfile({ mode: m.id })}
          >
            <Icon name={m.icon} size={20} />
            <span className="locaryn-mode-card-text">
              <strong>{m.label}</strong>
              <span>{m.hint}</span>
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
