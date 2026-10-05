import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type WorkspaceChanges, core } from "../../lib/core";

type Props = { sessionId: string | null; visible: boolean };

/** Le code de `git status`, en mots. */
function etatLisible(code: string): string {
  if (code === "??") return "nouveau";
  if (code.includes("A")) return "ajouté";
  if (code.includes("D")) return "supprimé";
  if (code.includes("R")) return "renommé";
  if (code.includes("U")) return "en conflit";
  return "modifié";
}

/** La classe de couleur d'un état, en ASCII. */
function classeEtat(code: string): string {
  if (code === "??") return "nouveau";
  if (code.includes("A")) return "ajoute";
  if (code.includes("D")) return "supprime";
  if (code.includes("U")) return "conflit";
  return "modifie";
}

/** Une ligne de diff et sa couleur. */
function classeLigne(ligne: string): string {
  if (ligne.startsWith("+++") || ligne.startsWith("---")) return "lw-diff-meta";
  if (ligne.startsWith("@@")) return "lw-diff-hunk";
  if (ligne.startsWith("+")) return "lw-diff-add";
  if (ligne.startsWith("-")) return "lw-diff-del";
  return "";
}

/**
 * Ce qui a changé dans le dépôt de la conversation : la liste des fichiers,
 * puis le diff de celui qu'on ouvre. Relue chaque fois que l'onglet revient à
 * l'écran — c'est souvent le modèle qui vient d'écrire.
 */
export function ChangesTab({ sessionId, visible }: Props) {
  const [changes, setChanges] = useState<WorkspaceChanges | null>(null);
  const [ouvert, setOuvert] = useState<string | null>(null);
  const [diff, setDiff] = useState<string | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [chargement, setChargement] = useState(false);

  const relire = useCallback(async () => {
    if (!sessionId) return;
    setChargement(true);
    setErreur(null);
    try {
      setChanges(await core.workspaceChanges(sessionId));
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setChargement(false);
    }
  }, [sessionId]);

  useEffect(() => {
    if (visible) void relire();
  }, [visible, relire]);

  async function montrer(chemin: string) {
    if (!sessionId) return;
    if (ouvert === chemin) {
      setOuvert(null);
      setDiff(null);
      return;
    }
    setOuvert(chemin);
    setDiff(null);
    try {
      setDiff(await core.workspaceDiff(sessionId, chemin));
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    }
  }

  if (!sessionId) {
    return (
      <div className="lw-empty">
        <Icon name="git-branch" size={22} />
        <p>Ouvrez une conversation pour voir ses modifications.</p>
      </div>
    );
  }

  return (
    <div className="lw-changes">
      <div className="lw-changes-head">
        <span className="lw-changes-branch">
          <Icon name="git-branch" size={14} />
          {changes?.depot ? (changes.branche ?? "branche détachée") : "—"}
        </span>
        <span className="lw-changes-count">
          {changes?.depot ? `${changes.fichiers.length} fichier(s)` : ""}
        </span>
        <button
          type="button"
          className="lw-icon-btn"
          aria-label="Relire"
          title="Relire"
          onClick={() => void relire()}
        >
          <span className={chargement ? "locaryn-spin lw-spin" : "lw-spin"}>
            <Icon name="arrow-clockwise" size={14} />
          </span>
        </button>
      </div>

      {erreur && (
        <p className="lw-browser-error" role="alert">
          {erreur}
        </p>
      )}

      {changes && !changes.depot && (
        <div className="lw-empty">
          <Icon name="git-branch" size={22} />
          <p>Ce dossier n'est pas un dépôt git : il n'y a pas d'historique à comparer.</p>
          <code className="lw-empty-path">{changes.racine}</code>
        </div>
      )}

      {changes?.depot && changes.fichiers.length === 0 && (
        <div className="lw-empty">
          <Icon name="check-circle" size={22} />
          <p>Aucune modification depuis le dernier commit.</p>
        </div>
      )}

      {changes?.depot && changes.fichiers.length > 0 && (
        <ul className="lw-entries">
          {changes.fichiers.map((f) => (
            <li key={f.chemin}>
              <button
                type="button"
                className={`lw-entry${ouvert === f.chemin ? " lw-entry-open" : ""}`}
                aria-expanded={ouvert === f.chemin}
                onClick={() => void montrer(f.chemin)}
              >
                <span className={`lw-change-tag lw-change-${classeEtat(f.etat)}`}>
                  {etatLisible(f.etat)}
                </span>
                <span className="lw-entry-name" title={f.chemin}>
                  {f.chemin}
                </span>
                {f.ajouts != null && <span className="lw-diff-add">+{f.ajouts}</span>}
                {f.retraits != null && <span className="lw-diff-del">−{f.retraits}</span>}
              </button>
              {ouvert === f.chemin && (
                <pre className="lw-diff">
                  {diff === null
                    ? "Lecture…"
                    : diff.trim() === ""
                      ? "(aucune différence de texte)"
                      : diff.split("\n").map((l, i) => (
                          // biome-ignore lint/suspicious/noArrayIndexKey: lignes d'un diff figé, deux lignes identiques sont courantes.
                          <span key={i} className={classeLigne(l)}>
                            {l}
                            {"\n"}
                          </span>
                        ))}
                </pre>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
