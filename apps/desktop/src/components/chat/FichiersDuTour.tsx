import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import { estPageHtml } from "../../lib/apercuPage";
import type { WorkspaceEntry } from "../../lib/core";
import { executerFichier } from "../../lib/executer";
import { ouvrirFichier } from "../../lib/workspace";

function taille(octets: number): string {
  if (octets < 1024) return `${octets} o`;
  if (octets < 1024 * 1024) return `${(octets / 1024).toFixed(1)} Ko`;
  return `${(octets / 1024 / 1024).toFixed(1)} Mo`;
}

/** Une application web que la réponse a créée ou modifiée. */
type Artefact = {
  /** Le fichier qui la lance : sa page, ou le `package.json` du projet. */
  chemin: string;
  titre: string;
  genre: "page" | "projet";
};

function dossierDe(chemin: string): string {
  const i = chemin.lastIndexOf("/");
  return i < 0 ? "" : chemin.slice(0, i);
}

function dans(chemin: string, dossier: string): boolean {
  return dossier === "" || chemin === dossier || chemin.startsWith(`${dossier}/`);
}

/**
 * Les applications web parmi les fichiers du tour : un projet par
 * `package.json`, une page par dossier (son `index.html` de préférence)
 * hors des projets — leur page passe par le serveur de développement.
 */
function artefactsWeb(fichiers: WorkspaceEntry[]): Artefact[] {
  const projets = fichiers
    .filter((f) => f.nom === "package.json" && !f.chemin.includes("node_modules/"))
    .map((f): Artefact => {
      const dossier = dossierDe(f.chemin);
      return { chemin: f.chemin, titre: dossier.split("/").pop() || "Projet", genre: "projet" };
    });
  const dossiersProjets = projets.map((p) => dossierDe(p.chemin));
  const pages = new Map<string, WorkspaceEntry>();
  for (const f of fichiers) {
    if (!estPageHtml(f.chemin)) continue;
    const dossier = dossierDe(f.chemin);
    if (dossiersProjets.some((d) => dans(dossier, d))) continue;
    const deja = pages.get(dossier);
    if (!deja || f.nom.toLowerCase() === "index.html") pages.set(dossier, f);
  }
  const statiques = [...pages.entries()].map(
    ([dossier, f]): Artefact => ({
      chemin: f.chemin,
      titre:
        f.nom.toLowerCase() === "index.html" && dossier
          ? (dossier.split("/").pop() ?? f.nom)
          : f.nom,
      genre: "page",
    }),
  );
  return [...projets, ...statiques];
}

/**
 * Une application web en carte, comme un artefact : son nom, ce qu'elle est,
 * « Ouvrir l'aperçu » qui l'affiche dans le panneau de droite de la
 * conversation (le serveur de développement d'abord, pour un projet), et
 * « Code » qui montre son fichier.
 */
function CarteArtefact({ artefact }: { artefact: Artefact }) {
  const [enCours, setEnCours] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);

  async function ouvrir() {
    setEnCours(true);
    setErreur(null);
    try {
      await executerFichier(null, artefact.chemin);
    } catch (e) {
      setErreur(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setEnCours(false);
    }
  }

  return (
    <div className="locaryn-artefact">
      <span className="locaryn-artefact-icone" aria-hidden="true">
        <Icon name={artefact.genre === "projet" ? "cube" : "globe"} size={18} />
      </span>
      <span className="locaryn-artefact-texte">
        <span className="locaryn-artefact-titre">{artefact.titre}</span>
        <span className="locaryn-artefact-genre">
          {artefact.genre === "projet" ? "Application web · serveur de dev" : "Page web"}
          {" · "}
          {artefact.chemin}
        </span>
        {erreur && (
          <span className="locaryn-artefact-erreur" role="alert">
            {erreur}
          </span>
        )}
      </span>
      <span className="locaryn-artefact-actions">
        <button
          type="button"
          className="locaryn-artefact-btn"
          onClick={() => ouvrirFichier(artefact.chemin, true)}
          title={`Voir le code de ${artefact.chemin}`}
        >
          <Icon name="code" size={14} /> Code
        </button>
        <button
          type="button"
          className="locaryn-artefact-btn is-principal"
          onClick={() => void ouvrir()}
          disabled={enCours}
          aria-busy={enCours}
        >
          <span className={enCours ? "locaryn-spin" : undefined}>
            <Icon name={enCours ? "circle-notch" : "eye"} size={14} />
          </span>
          Ouvrir l'aperçu
        </button>
      </span>
    </div>
  );
}

/**
 * Les fichiers que la réponse vient de créer ou de modifier, sous elle.
 *
 * Qu'il les ait écrits avec `write_file`, une commande ou un connecteur, la
 * personne les retrouve ici d'un clic : le fichier s'ouvre dans le panneau
 * Fichiers, avec son aperçu et les boutons pour l'ouvrir ailleurs. Une
 * application web a en plus sa carte : rien ne s'ouvre sans qu'on le demande.
 */
export function FichiersDuTour({ fichiers }: { fichiers: WorkspaceEntry[] }) {
  if (fichiers.length === 0) return null;
  const artefacts = artefactsWeb(fichiers);
  return (
    <div className="locaryn-fichiers-tour">
      {artefacts.map((a) => (
        <CarteArtefact key={a.chemin} artefact={a} />
      ))}
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
