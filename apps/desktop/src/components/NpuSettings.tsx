import { useEffect, useState } from "react";
import { type EtatNpu, core } from "../lib/core";

/**
 * Le NPU Hexagon des PC Snapdragon. Locaryn l'essaie en premier ; s'il
 * refuse de démarrer, le GPU Adreno prend le relais et la raison s'affiche
 * ici, avec la marche à suivre pour l'autoriser.
 */
export function NpuSettings() {
  const [etat, setEtat] = useState<EtatNpu | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [occupe, setOccupe] = useState(false);

  useEffect(() => {
    core
      .npuEtat()
      .then(setEtat)
      .catch(() => {
        // Pas d'état lisible : la carte reste cachée.
      });
  }, []);

  if (!etat?.snapdragon) return null;

  async function faire(action: () => Promise<EtatNpu>) {
    setOccupe(true);
    setErreur(null);
    try {
      setEtat(await action());
    } catch (e) {
      setErreur(e instanceof Error ? e.message : String(e));
    } finally {
      setOccupe(false);
    }
  }

  const statut = !etat.voulu
    ? "Coupé : le GPU Adreno est utilisé."
    : etat.echec
      ? "Le NPU n'a pas démarré : le GPU Adreno est utilisé à la place."
      : etat.installe
        ? "Actif : les modèles se chargent sur le NPU."
        : "Prêt : le moteur NPU se télécharge au prochain chargement d'un modèle.";

  return (
    <div className="locaryn-field">
      <span className="locaryn-field-label">NPU Snapdragon (Hexagon)</span>
      <p className="locaryn-field-hint">
        Le NPU lit les prompts bien plus vite que le processeur, sans occuper le GPU. Il accélère
        les modèles en Q4_0, Q8_0, IQ4_NL ou MXFP4 : choisissez de préférence ces versions.
      </p>
      <div className="locaryn-decisions-ligne">
        <span className={`locaryn-decisions-etat${etat.voulu && !etat.echec ? " is-ok" : ""}`}>
          {statut}
        </span>
        <button
          type="button"
          className="locaryn-btn"
          disabled={occupe}
          onClick={() => void faire(() => core.npuRegler(!etat.voulu))}
        >
          {etat.voulu ? "Couper le NPU" : "Utiliser le NPU"}
        </button>
      </div>
      {etat.voulu && etat.echec && (
        <div className="locaryn-npu-aide">
          <p className="locaryn-field-hint">Raison : {etat.echec}</p>
          <p className="locaryn-field-hint">
            Windows n'accepte les bibliothèques du NPU que si leur signature est autorisée. Une
            seule fois : ouvrez le dossier du moteur, lancez <code>activer-npu.ps1</code> dans un
            PowerShell administrateur, redémarrez le PC, puis cliquez sur « Réessayer le NPU ».
          </p>
          <div className="locaryn-field-actions">
            {etat.installe && (
              <button
                type="button"
                className="locaryn-btn"
                onClick={() =>
                  void core
                    .ouvrirChemin(etat.dossier, "explorer")
                    .catch((e) => setErreur(String(e)))
                }
              >
                Ouvrir le dossier du moteur
              </button>
            )}
            <button
              type="button"
              className="locaryn-btn-primary"
              disabled={occupe}
              onClick={() => void faire(() => core.npuReessayer())}
            >
              Réessayer le NPU
            </button>
          </div>
        </div>
      )}
      {erreur && <p className="locaryn-field-error">{erreur}</p>}
    </div>
  );
}
