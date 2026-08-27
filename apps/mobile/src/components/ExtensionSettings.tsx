import { LoSwitch } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { type SettingsField, type SettingsSection, api } from "../lib/core";

/** Une section, et l'extension qui l'apporte. */
type Apportee = { section: SettingsSection; extension: string };

/**
 * Les réglages que les extensions ajoutent.
 *
 * Une extension de dictée doit pouvoir faire choisir son modèle de
 * reconnaissance, et ce choix doit se faire depuis le téléphone comme depuis
 * l'ordinateur — c'est le même serveur qui exécutera. Le manifeste décrit les
 * champs, l'application les dessine ; rien n'est codé en dur pour une
 * extension en particulier.
 *
 * Les valeurs vivent sur le serveur, avec l'extension. Le téléphone n'en garde
 * pas de copie : deux appareils qui divergeraient sur le même réglage seraient
 * pires que pas de réglage du tout.
 */
export function ExtensionSettings() {
  const [sections, setSections] = useState<Apportee[]>([]);
  const [valeurs, setValeurs] = useState<Record<string, string>>({});
  const [modeles, setModeles] = useState<string[]>([]);
  const [enregistre, setEnregistre] = useState<string | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const exts = await api.listExtensions();
        const apportees = exts.flatMap((e) =>
          (e.ui?.settings_sections ?? []).map((section) => ({
            section,
            extension: e.name,
          })),
        );
        setSections(apportees);
        if (apportees.length > 0) {
          const [config, mods] = await Promise.all([
            api.extensionConfig().catch(() => ({}) as Record<string, string>),
            api.listModels().catch(() => [] as string[]),
          ]);
          setValeurs(config);
          setModeles(mods);
        }
      } catch (e) {
        setErreur(String(e));
      }
    })();
  }, []);

  if (sections.length === 0) return null;

  async function ecrire(extension: string, key: string, valeur: string) {
    const plein = `${extension}.${key}`;
    setValeurs((v) => ({ ...v, [plein]: valeur }));
    try {
      await api.setExtensionConfig(extension, key, valeur);
      setEnregistre(plein);
      setErreur(null);
      window.setTimeout(() => setEnregistre((e) => (e === plein ? null : e)), 1500);
    } catch (e) {
      setErreur(String(e));
    }
  }

  function champ(extension: string, f: SettingsField) {
    const plein = `${extension}.${f.key}`;
    const valeur = valeurs[plein] ?? f.default ?? "";
    // Le vocabulaire canonique : boolean, select, model, string, number,
    // prompt. Les anciens mots (toggle, choice, text) restent acceptés.
    const kind =
      f.kind === "toggle"
        ? "boolean"
        : f.kind === "choice"
          ? "select"
          : f.kind === "text"
            ? "string"
            : f.kind;

    if (kind === "boolean") {
      return (
        <LoSwitch
          checked={valeur === "true"}
          onChange={(actif) => void ecrire(extension, f.key, actif ? "true" : "false")}
          label={f.label ?? f.key}
        />
      );
    }

    const choix = kind === "model" ? modeles : (f.options ?? []);
    if (kind === "select" || kind === "model") {
      return (
        <select
          className="lo-select"
          value={valeur}
          onChange={(e) => void ecrire(extension, f.key, e.target.value)}
        >
          <option value="">Aucun</option>
          {choix.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      );
    }

    if (kind === "prompt") {
      return (
        <textarea
          className="lo-textarea"
          rows={4}
          value={valeur}
          onChange={(e) => setValeurs((v) => ({ ...v, [plein]: e.target.value }))}
          onBlur={(e) => void ecrire(extension, f.key, e.target.value)}
        />
      );
    }

    return (
      <input
        className="lo-input"
        value={valeur}
        onChange={(e) => setValeurs((v) => ({ ...v, [plein]: e.target.value }))}
        onBlur={(e) => void ecrire(extension, f.key, e.target.value)}
      />
    );
  }

  return (
    <>
      {sections.map(({ section, extension }) => (
        <section className="lo-section" key={`${extension}.${section.id}`}>
          <h2 className="lo-section-title">{section.title}</h2>
          {section.description && <p className="lo-hint">{section.description}</p>}
          {section.fields.map((f) => (
            <div className="lo-card" key={f.key}>
              <div className="lo-card-text">
                <span className="lo-card-title">{f.label}</span>
                {f.hint && <span className="lo-hint">{f.hint}</span>}
                {enregistre === `${extension}.${f.key}` && (
                  <span className="lo-hint">Enregistré</span>
                )}
              </div>
              <div className="lo-card-actions">{champ(extension, f)}</div>
            </div>
          ))}
        </section>
      ))}
      {erreur && <p className="lo-error">{erreur}</p>}
    </>
  );
}
