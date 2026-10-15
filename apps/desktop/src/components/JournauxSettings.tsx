import { LoSwitch } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { core } from "../lib/core";
import { ecrirePressePapiers } from "../lib/pressePapiers";

/**
 * Les journaux de l'application, à transmettre quand quelque chose ne va pas.
 *
 * « Voir les journaux » rassemble en un bloc la machine, la fin de chaque
 * journal et les plantages — dossier personnel, clés et jetons masqués — à
 * copier dans Discord ou ailleurs. Quand cette version connaît un point de
 * collecte, la personne peut aussi l'envoyer d'un clic, ou cocher l'envoi
 * automatique des rapports d'erreur.
 */
export function JournauxSettings() {
  const [ouvert, setOuvert] = useState(false);
  const [texte, setTexte] = useState<string | null>(null);
  const [rapports, setRapports] = useState<{ disponible: boolean; actif: boolean } | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [occupe, setOccupe] = useState(false);

  useEffect(() => {
    core
      .rapportsEtat()
      .then(setRapports)
      .catch(() => {
        // Sans état lisible : pas d'envoi proposé, la copie reste.
      });
  }, []);

  async function afficher() {
    setOuvert(true);
    setErreur(null);
    try {
      setTexte(await core.journauxRassembler());
    } catch (e) {
      setErreur(e instanceof Error ? e.message : String(e));
    }
  }

  async function copier() {
    if (!texte) return;
    try {
      await ecrirePressePapiers(texte);
      setMessage("Journaux copiés : collez-les dans votre message (Discord, e-mail…).");
    } catch (e) {
      setErreur(e instanceof Error ? e.message : String(e));
    }
  }

  async function envoyer() {
    setOccupe(true);
    setErreur(null);
    try {
      await core.rapportsEnvoyer(null);
      setMessage("Journaux envoyés à l'équipe Locaryn. Merci !");
    } catch (e) {
      setErreur(e instanceof Error ? e.message : String(e));
    } finally {
      setOccupe(false);
    }
  }

  return (
    <>
      <h3 style={{ marginTop: 24 }}>Journaux et rapports</h3>
      <p className="locaryn-field-hint">
        En cas de problème, les journaux disent ce qui s'est passé. Votre dossier personnel, vos
        clés et vos jetons y sont masqués.
      </p>
      <div className="locaryn-field-actions">
        <button
          type="button"
          className="locaryn-btn-ghost"
          style={{ fontSize: 12 }}
          onClick={() => void (ouvert ? setOuvert(false) : afficher())}
        >
          {ouvert ? "Masquer les journaux" : "Voir les journaux"}
        </button>
        <button
          type="button"
          className="locaryn-btn-ghost"
          style={{ fontSize: 12 }}
          onClick={() =>
            void core
              .journauxDossier()
              .then((d) => core.ouvrirChemin(d, "explorer"))
              .catch((e) => setErreur(String(e)))
          }
        >
          Ouvrir le dossier
        </button>
      </div>

      {ouvert && (
        <div className="locaryn-journaux">
          <pre className="locaryn-journaux-texte" data-select-scope>
            {texte ?? "Lecture…"}
          </pre>
          <div className="locaryn-field-actions">
            <button
              type="button"
              className="locaryn-btn-primary"
              disabled={!texte}
              onClick={() => void copier()}
            >
              Copier
            </button>
            {rapports?.disponible && (
              <button
                type="button"
                className="locaryn-btn"
                disabled={occupe}
                onClick={() => void envoyer()}
              >
                {occupe ? "Envoi…" : "Envoyer à l'équipe"}
              </button>
            )}
            <button type="button" className="locaryn-btn-ghost" onClick={() => void afficher()}>
              Actualiser
            </button>
          </div>
        </div>
      )}

      {rapports?.disponible && (
        <div className="locaryn-field" style={{ marginTop: 12 }}>
          <div className="locaryn-decisions-ligne">
            <span className="locaryn-field-label" style={{ margin: 0 }}>
              Envoyer automatiquement les rapports d'erreur
            </span>
            <LoSwitch
              checked={rapports.actif}
              onChange={(actif) => {
                void core
                  .rapportsRegler(actif)
                  .then(setRapports)
                  .catch((e) => setErreur(String(e)));
              }}
            />
          </div>
          <p className="locaryn-field-hint">
            Un plantage, un moteur qui ne démarre pas ou une conversation qui échoue envoient le
            résumé de l'erreur et les journaux (masqués) à l'équipe Locaryn, pour corriger plus
            vite. Jamais vos conversations ni vos fichiers.
          </p>
        </div>
      )}

      {message && !erreur && <p className="locaryn-field-hint">{message}</p>}
      {erreur && <p className="locaryn-field-error">{erreur}</p>}
    </>
  );
}
