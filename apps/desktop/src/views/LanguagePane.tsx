import { Icon } from "@locaryn/ui-core";
import { DO_NOT_TRANSLATE, LANGUAGES, useI18n } from "../lib/i18n";

/**
 * Langue de l'interface.
 *
 * Vivait dans la section « Langue » des réglages ; rejoint le compte, à côté
 * de l'apparence — la langue est une préférence de profil, pas un réglage
 * d'infrastructure.
 */
export function LanguagePane() {
  const { lang, setLang } = useI18n();
  return (
    <div className="locaryn-field">
      <div className="locaryn-field-label">Langue de l'interface</div>
      <p className="locaryn-field-hint">
        Change la langue des textes de l'application. Les noms de modèles, de marques et les termes
        techniques restent inchangés.
      </p>
      <div className="locaryn-lang-grid" style={{ marginTop: 12 }}>
        {LANGUAGES.map((l) => (
          <button
            key={l.id}
            type="button"
            className={`locaryn-lang-item${lang === l.id ? " locaryn-active" : ""}`}
            onClick={() => setLang(l.id)}
          >
            <span className="locaryn-lang-flag">{l.flag}</span>
            <span>{l.label}</span>
            {lang === l.id && (
              <span className="locaryn-lang-check">
                <Icon name="check" size={14} />
              </span>
            )}
          </button>
        ))}
      </div>
      <p className="locaryn-field-hint" style={{ marginTop: 16 }}>
        <strong>Jamais traduits</strong> — noms de produits et termes techniques :
      </p>
      <div className="locaryn-lang-terms">
        {DO_NOT_TRANSLATE.slice(0, 14).map((w) => (
          <code key={w}>{w}</code>
        ))}
        <span className="locaryn-field-hint">+{DO_NOT_TRANSLATE.length - 14} autres</span>
      </div>
    </div>
  );
}
