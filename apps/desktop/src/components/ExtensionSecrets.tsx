import { useCallback, useEffect, useState } from "react";
import { type SecretDeclare, type SecretInfo, core } from "../lib/core";

/** Le JSON d'une réponse d'outil MCP, qu'il arrive nu ou dans `content[].text`. */
function lireJson(brut: string): unknown {
  try {
    const v = JSON.parse(brut) as unknown;
    const texte = (v as { content?: { text?: string }[] })?.content?.[0]?.text;
    if (typeof texte === "string") {
      try {
        return JSON.parse(texte) as unknown;
      } catch {
        return v;
      }
    }
    return v;
  } catch {
    return null;
  }
}

type Ligne = { nom: string; titre: string; variante: string | null; description?: string };

/** Les noms concrets d'un secret déclaré : un par variante (`wsl-sudo-{name}`). */
async function lignesDe(d: SecretDeclare): Promise<Ligne[]> {
  const titre = d.title ?? d.name;
  if (!d.variants?.tool || !/\{[\w.-]+\}/.test(d.name)) {
    return [{ nom: d.name, titre, variante: null, description: d.description }];
  }
  const sortie = lireJson(await core.invokeExtensionTool(d.variants.tool, {}));
  const liste = d.variants.list
    ? (sortie as Record<string, unknown> | null)?.[d.variants.list]
    : sortie;
  if (!Array.isArray(liste)) return [];
  const champ = d.variants.field ?? "name";
  return liste
    .map((x) => (typeof x === "string" ? x : (x as Record<string, unknown>)?.[champ]))
    .filter((v): v is string => typeof v === "string" && v.length > 0)
    .map((v) => ({
      nom: d.name.replace(/\{[\w.-]+\}/, v),
      titre: `${titre} — ${v}`,
      variante: v,
      description: d.description,
    }));
}

/**
 * Les secrets qu'une extension sait utiliser, gérés par le coffre natif.
 *
 * L'extension les déclare dans son manifeste (`secrets`) ; quand ils dépendent
 * d'une liste (une par distribution WSL), elle donne l'outil qui la fournit.
 * La valeur se tape ici, une fois : elle va au trousseau du système, ni dans
 * les réglages de l'extension, ni au modèle.
 */
export function ExtensionSecrets({ declares }: { declares: SecretDeclare[] }) {
  const [lignes, setLignes] = useState<Ligne[] | null>(null);
  const [ranges, setRanges] = useState<SecretInfo[]>([]);
  const [saisies, setSaisies] = useState<Record<string, string>>({});
  const [erreur, setErreur] = useState<string | null>(null);

  const relire = useCallback(() => {
    void core
      .secretsLister()
      .then(setRanges)
      .catch(() => setRanges([]));
  }, []);

  useEffect(() => {
    let vivant = true;
    void Promise.all(declares.map(lignesDe))
      .then((l) => {
        if (vivant) setLignes(l.flat());
      })
      .catch((e) => {
        if (vivant) {
          setLignes([]);
          setErreur(String(e));
        }
      });
    relire();
    return () => {
      vivant = false;
    };
  }, [declares, relire]);

  async function ranger(l: Ligne) {
    setErreur(null);
    try {
      await core.secretDefinir(l.nom, l.titre, saisies[l.nom] ?? "");
      setSaisies((s) => ({ ...s, [l.nom]: "" }));
      relire();
    } catch (e) {
      setErreur(String(e));
    }
  }

  async function effacer(nom: string) {
    setErreur(null);
    try {
      await core.secretEffacer(nom);
      relire();
    } catch (e) {
      setErreur(String(e));
    }
  }

  return (
    <div className="locaryn-coffre">
      <p className="locaryn-field-hint">
        Rangés dans le trousseau du système. Le modèle s'en sert sans jamais les voir ; il peut
        aussi vous les demander dans la conversation, dans un champ masqué.
      </p>
      {lignes === null && <p className="locaryn-field-hint">Lecture…</p>}
      {lignes && lignes.length === 0 && (
        <p className="locaryn-field-hint">Rien à configurer pour l'instant.</p>
      )}
      {lignes && lignes.length > 0 && (
        <ul className="locaryn-coffre-liste">
          {lignes.map((l) => {
            const range = ranges.some((r) => r.nom === l.nom);
            return (
              <li key={l.nom} className="locaryn-coffre-ligne">
                <span className="locaryn-coffre-texte">
                  <span className="locaryn-coffre-libelle">{l.titre}</span>
                  <code className="locaryn-coffre-nom">{l.nom}</code>
                </span>
                {range ? (
                  <>
                    <span className="locaryn-coffre-valeur is-range">Enregistré</span>
                    <button
                      type="button"
                      className="locaryn-btn-ghost"
                      onClick={() => void effacer(l.nom)}
                    >
                      Effacer
                    </button>
                  </>
                ) : (
                  <>
                    <input
                      className="locaryn-input locaryn-coffre-champ"
                      type="password"
                      autoComplete="off"
                      placeholder="Mot de passe"
                      value={saisies[l.nom] ?? ""}
                      onChange={(e) => setSaisies((s) => ({ ...s, [l.nom]: e.target.value }))}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && saisies[l.nom]) void ranger(l);
                      }}
                    />
                    <button
                      type="button"
                      className="locaryn-btn-primary"
                      disabled={!saisies[l.nom]}
                      onClick={() => void ranger(l)}
                    >
                      Ranger
                    </button>
                  </>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {erreur && (
        <p className="locaryn-field-error" role="alert">
          {erreur}
        </p>
      )}
    </div>
  );
}
