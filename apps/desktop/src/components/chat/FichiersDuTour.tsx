import { Icon } from "@locaryn/ui-core";
import type { WorkspaceEntry } from "../../lib/core";
import { ouvrirFichier } from "../../lib/workspace";

function taille(octets: number): string {
  if (octets < 1024) return `${octets} o`;
  if (octets < 1024 * 1024) return `${(octets / 1024).toFixed(1)} Ko`;
  return `${(octets / 1024 / 1024).toFixed(1)} Mo`;
}

/**
 * Les fichiers que la réponse vient de créer ou de modifier, sous elle.
 *
 * Qu'il les ait écrits avec `write_file`, une commande ou un connecteur, la
 * personne les retrouve ici d'un clic : le fichier s'ouvre dans le panneau
 * Fichiers, avec son aperçu et les boutons pour l'ouvrir ailleurs.
 */
export function FichiersDuTour({ fichiers }: { fichiers: WorkspaceEntry[] }) {
  if (fichiers.length === 0) return null;
  return (
    <div className="locaryn-fichiers-tour">
      <span className="locaryn-fichiers-tour-titre">
        <Icon name="file-text" size={13} />
        {fichiers.length === 1 ? "Fichier créé ou modifié" : "Fichiers créés ou modifiés"}
      </span>
      <div className="locaryn-fichiers-tour-liste">
        {fichiers.map((f) => (
          <button
            key={f.chemin}
            type="button"
            className="locaryn-fichier-puce"
            title={`Ouvrir ${f.chemin} dans le panneau Fichiers`}
            onClick={() => ouvrirFichier(f.chemin)}
          >
            <span className="locaryn-fichier-puce-nom">{f.nom}</span>
            {f.chemin !== f.nom && (
              <span className="locaryn-fichier-puce-dossier">
                {f.chemin.slice(0, f.chemin.length - f.nom.length - 1)}
              </span>
            )}
            <span className="locaryn-fichier-puce-taille">{taille(f.taille)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
