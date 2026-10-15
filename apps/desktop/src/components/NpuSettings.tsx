import { useEffect, useState } from "react";
import { type EtatNpu, core } from "../lib/core";
import { ecrirePressePapiers } from "../lib/pressePapiers";

function mio(n: number): string {
  return n >= 1024 ? `${(n / 1024).toFixed(1)} Go` : `${Math.round(n)} Mo`;
}

/**
 * Le NPU Hexagon des PC Snapdragon. Locaryn l'essaie en premier ; s'il
 * refuse de démarrer, le GPU Adreno prend le relais et la raison s'affiche
 * ici, avec la marche à suivre pour l'autoriser. Quand il tourne, la carte
 * dit combien du modèle il porte, et si son format est calculé par le NPU.
 */
export function NpuSettings() {
  const [etat, setEtat] = useState<EtatNpu | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [occupe, setOccupe] = useState<null | "regler" | "convertir" | "diag">(null);

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
    setOccupe("regler");
    setErreur(null);
    try {
      setEtat(await action());
    } catch (e) {
      setErreur(e instanceof Error ? e.message : String(e));
    } finally {
      setOccupe(null);
    }
  }

  async function copierDiagnostic() {
    setOccupe("diag");
    try {
      await ecrirePressePapiers(await core.npuDiagnostic());
      setMessage("Diagnostic copié : collez-le dans votre message.");
    } catch (e) {
      setErreur(e instanceof Error ? e.message : String(e));
    } finally {
      setOccupe(null);
    }
  }

  async function convertir() {
    if (!etat?.modele) return;
    setOccupe("convertir");
    setErreur(null);
    setMessage("Conversion en Q4_0… (une à quelques minutes)");
    try {
      const nouveau = await core.npuConvertir(etat.modele);
      setMessage("Version Q4_0 créée : chargement sur le NPU…");
      await core.loadChatModel(nouveau, false);
      setEtat(await core.npuEtat());
      setMessage(`Chargé : ${nouveau}`);
    } catch (e) {
      setMessage(null);
      setErreur(e instanceof Error ? e.message : String(e));
    } finally {
      setOccupe(null);
    }
  }

  const actif = etat.voulu && !etat.echec && etat.actif;
  const statut = !etat.voulu
    ? "Coupé : le GPU Adreno est utilisé."
    : etat.echec
      ? "Le NPU n'a pas démarré : le GPU Adreno est utilisé à la place."
      : actif
        ? `Actif : ${mio(actif.npu_mib)} du modèle sur le NPU${actif.arch ? ` (Hexagon ${actif.arch})` : ""}, ${actif.sessions} session${actif.sessions > 1 ? "s" : ""}.`
        : etat.installe
          ? "Prêt : le prochain chargement d'un modèle se fera sur le NPU."
          : "Prêt : le moteur NPU se télécharge au prochain chargement d'un modèle.";

  return (
    <div className="locaryn-field">
      <span className="locaryn-field-label">NPU Snapdragon (Hexagon)</span>
      <p className="locaryn-field-hint">
        Le NPU lit les prompts bien plus vite que le processeur, sans occuper le GPU. Il calcule
        lui-même les modèles en Q4_0, Q8_0, IQ4_NL ou MXFP4 ; avec un autre format, les calculs
        repartent sur le processeur et le NPU reste à zéro.
      </p>
      <div className="locaryn-decisions-ligne">
        <span className={`locaryn-decisions-etat${actif ? " is-ok" : ""}`}>{statut}</span>
        <button
          type="button"
          className="locaryn-btn"
          disabled={occupe !== null}
          onClick={() => void faire(() => core.npuRegler(!etat.voulu))}
        >
          {etat.voulu ? "Couper le NPU" : "Utiliser le NPU"}
        </button>
      </div>

      {etat.voulu && etat.modele && !etat.format_accelere && (
        <div className="locaryn-npu-aide">
          <p className="locaryn-field-hint">
            Le modèle actif (<code>{etat.modele.split("/").pop()}</code>) n'est pas dans un format
            que le NPU calcule : il restera à zéro. Locaryn peut en créer une version Q4_0, à côté
            de l'original (un peu moins précise, bien plus rapide sur le NPU).
          </p>
          <div className="locaryn-field-actions">
            <button
              type="button"
              className="locaryn-btn-primary"
              disabled={occupe !== null || !etat.installe}
              onClick={() => void convertir()}
            >
              {occupe === "convertir" ? "Conversion…" : "Créer une version Q4_0 et la charger"}
            </button>
          </div>
        </div>
      )}

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
              disabled={occupe !== null}
              onClick={() => void faire(() => core.npuReessayer())}
            >
              Réessayer le NPU
            </button>
          </div>
        </div>
      )}

      <div className="locaryn-field-actions">
        <button
          type="button"
          className="locaryn-btn"
          disabled={occupe !== null}
          onClick={() => void copierDiagnostic()}
        >
          Copier le diagnostic
        </button>
      </div>
      {message && !erreur && <p className="locaryn-field-hint">{message}</p>}
      {erreur && <p className="locaryn-field-error">{erreur}</p>}
    </div>
  );
}
