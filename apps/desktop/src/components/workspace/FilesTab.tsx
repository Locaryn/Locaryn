import { Icon } from "@locaryn/ui-core";
import { useCallback, useEffect, useState } from "react";
import { type WorkspaceEntry, type WorkspaceFile, core } from "../../lib/core";

type Props = { sessionId: string | null; visible: boolean };

function taille(octets: number): string {
  if (octets < 1024) return `${octets} o`;
  if (octets < 1024 * 1024) return `${(octets / 1024).toFixed(1)} Ko`;
  return `${(octets / 1024 / 1024).toFixed(1)} Mo`;
}

/**
 * Le dossier de la conversation, en lecture : on y descend par les dossiers,
 * on remonte par le fil d'Ariane, un fichier s'ouvre à la place de la liste.
 * Une liste plutôt qu'un arbre : le panneau est étroit, et un arbre déplié sur
 * trois niveaux n'y laisse plus de place aux noms.
 */
export function FilesTab({ sessionId, visible }: Props) {
  const [dossier, setDossier] = useState("");
  const [entrees, setEntrees] = useState<WorkspaceEntry[] | null>(null);
  const [fichier, setFichier] = useState<WorkspaceFile | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [chargement, setChargement] = useState(false);

  const charger = useCallback(
    async (chemin: string) => {
      if (!sessionId) return;
      setChargement(true);
      setErreur(null);
      try {
        setEntrees(await core.workspaceList(sessionId, chemin));
        setDossier(chemin);
        setFichier(null);
      } catch (e) {
        setErreur(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setChargement(false);
      }
    },
    [sessionId],
  );

  // Une autre conversation, ou l'onglet qui revient à l'écran : on relit, le
  // modèle a pu écrire entre-temps.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `dossier` et `fichier` sont lus, pas suivis — naviguer passe déjà par `charger`, les suivre ferait deux lectures par clic.
  useEffect(() => {
    if (visible && !fichier) void charger(dossier);
  }, [visible, charger]);

  function ouvrir(e: WorkspaceEntry) {
    if (e.dossier) void charger(e.chemin);
    else void lireFichier(e.chemin);
  }

  async function lireFichier(chemin: string) {
    if (!sessionId) return;
    setChargement(true);
    setErreur(null);
    try {
      setFichier(await core.workspaceRead(sessionId, chemin));
    } catch (err) {
      setErreur(String(err).replace(/^Error:\s*/, ""));
    } finally {
      setChargement(false);
    }
  }

  if (!sessionId) {
    return (
      <div className="lw-empty">
        <Icon name="folder-open" size={22} />
        <p>Ouvrez une conversation pour voir son dossier.</p>
      </div>
    );
  }

  const morceaux = dossier ? dossier.split("/") : [];

  return (
    <div className="lw-files">
      <nav className="lw-crumbs" aria-label="Emplacement">
        <button type="button" className="lw-crumb" onClick={() => void charger("")}>
          Dossier
        </button>
        {morceaux.map((m, i) => {
          const chemin = morceaux.slice(0, i + 1).join("/");
          return (
            <span key={chemin} className="lw-crumb-part">
              <span aria-hidden="true">/</span>
              <button type="button" className="lw-crumb" onClick={() => void charger(chemin)}>
                {m}
              </button>
            </span>
          );
        })}
        {fichier && (
          <span className="lw-crumb-part">
            <span aria-hidden="true">/</span>
            <span className="lw-crumb lw-crumb-current">{fichier.chemin.split("/").pop()}</span>
          </span>
        )}
        <button
          type="button"
          className="lw-icon-btn lw-crumbs-refresh"
          aria-label="Relire"
          title="Relire"
          onClick={() => (fichier ? void lireFichier(fichier.chemin) : void charger(dossier))}
        >
          <span className={chargement ? "locaryn-spin lw-spin" : "lw-spin"}>
            <Icon name="arrow-clockwise" size={14} />
          </span>
        </button>
      </nav>

      {erreur && (
        <p className="lw-browser-error" role="alert">
          {erreur}
        </p>
      )}

      {fichier ? (
        <div className="lw-file">
          <div className="lw-file-head">
            <button type="button" className="lw-link-btn" onClick={() => setFichier(null)}>
              <Icon name="arrow-left" size={14} /> Retour au dossier
            </button>
            <span className="lw-file-size">
              {taille(fichier.taille)}
              {fichier.tronque ? " · début seulement" : ""}
            </span>
          </div>
          {fichier.contenu === null ? (
            <div className="lw-empty">
              <Icon name="file-text" size={22} />
              <p>Fichier binaire : il ne s'affiche pas ici.</p>
            </div>
          ) : (
            <pre className="lw-file-code">{fichier.contenu}</pre>
          )}
        </div>
      ) : (
        <ul className="lw-entries">
          {entrees?.length === 0 && <li className="lw-entries-empty">Dossier vide.</li>}
          {entrees?.map((e) => (
            <li key={e.chemin}>
              <button type="button" className="lw-entry" onClick={() => ouvrir(e)}>
                <Icon name={e.dossier ? "folder-simple" : "file-text"} size={15} />
                <span className="lw-entry-name">{e.nom}</span>
                {!e.dossier && <span className="lw-entry-size">{taille(e.taille)}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
