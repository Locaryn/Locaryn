import { useEffect, useState } from "react";
import { type ReglagesGit, core } from "../lib/core";

/**
 * Le suivi Git du travail de l'agent : un dépôt créé dans un projet qui n'en a
 * pas, et Locaryn nommé co-auteur des commits qu'il fait. Les deux sont
 * allumés par défaut.
 */
export function GitSettings() {
  const [r, setR] = useState<ReglagesGit | null>(null);

  useEffect(() => {
    let vivant = true;
    core
      .gitReglages()
      .then((v) => {
        if (vivant) setR(v);
      })
      .catch(() => {
        if (vivant) setR({ init_auto: true, co_auteur: true });
      });
    return () => {
      vivant = false;
    };
  }, []);

  if (!r) return null;
  const appliquer = (suivant: ReglagesGit) => {
    setR(suivant);
    void core.setGitReglages(suivant).catch(() => setR(r));
  };

  const options: { cle: keyof ReglagesGit; titre: string; aide: string }[] = [
    {
      cle: "init_auto",
      titre: "Créer un dépôt Git dans les projets qui n'en ont pas",
      aide: "Dès que l'agent écrit un fichier, chaque modification se suit et s'annule. Jamais à la racine d'un disque ni dans votre dossier personnel.",
    },
    {
      cle: "co_auteur",
      titre: "Ajouter Locaryn en co-auteur des commits de l'agent",
      aide: "Les commits de l'agent portent « Co-Authored-By: Locaryn » : sur GitHub, on voit ce qui vient de lui. Vos propres commits ne changent pas.",
    },
  ];

  return (
    <div className="locaryn-field locaryn-git-reglages">
      <div className="locaryn-field-label">Suivi Git</div>
      <p className="locaryn-field-hint">Pour tous les projets.</p>
      <div className="locaryn-git-options">
        {options.map((o) => (
          <label key={o.cle} className="locaryn-git-option">
            <input
              type="checkbox"
              checked={r[o.cle]}
              onChange={(e) => appliquer({ ...r, [o.cle]: e.target.checked })}
            />
            <span className="locaryn-git-option-texte">
              <span className="locaryn-git-option-titre">{o.titre}</span>
              <span className="locaryn-field-hint">{o.aide}</span>
            </span>
          </label>
        ))}
      </div>
    </div>
  );
}
