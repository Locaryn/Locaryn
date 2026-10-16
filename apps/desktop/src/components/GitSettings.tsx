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

  return (
    <div className="locaryn-notif-settings">
      <h4 className="locaryn-settings-subtitle">Suivi Git</h4>
      <label className="locaryn-notif-groupe">
        <input
          type="checkbox"
          checked={r.init_auto}
          onChange={(e) => appliquer({ ...r, init_auto: e.target.checked })}
        />
        <span className="locaryn-notif-groupe-texte">
          <strong>Créer un dépôt Git dans les projets qui n'en ont pas</strong>
          <span className="locaryn-field-hint">
            Dès que l'agent écrit un fichier, chaque modification se suit et s'annule. Jamais à la
            racine d'un disque ni dans votre dossier personnel.
          </span>
        </span>
      </label>
      <label className="locaryn-notif-groupe">
        <input
          type="checkbox"
          checked={r.co_auteur}
          onChange={(e) => appliquer({ ...r, co_auteur: e.target.checked })}
        />
        <span className="locaryn-notif-groupe-texte">
          <strong>Ajouter Locaryn en co-auteur des commits de l'agent</strong>
          <span className="locaryn-field-hint">
            Les commits que fait l'agent portent « Co-Authored-By: Locaryn » : sur GitHub, on voit
            ce qui vient de lui. Vos propres commits ne changent pas.
          </span>
        </span>
      </label>
    </div>
  );
}
