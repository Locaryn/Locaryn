// La fenêtre qui dit pourquoi un modèle n'a pas été chargé.
//
// La conversation vérifie la prudence choisie (Réglages → Performance) avant
// d'envoyer. Un refus retient le message : il ne part que si la personne
// charge le modèle quand même, et revient dans le champ si elle renonce.

import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { type CautionLevel, type RefusDeChargement, core } from "../lib/core";
import { ModalShell } from "./ModalShell";

/** Le modèle a été chargé malgré la prudence : la conversation envoie le message retenu. */
export const MODELE_FORCE = "locaryn:modele-force";
/** La fenêtre s'est fermée sans charger : le message retenu revient dans le champ. */
export const MODELE_ANNULE = "locaryn:modele-annule";
/** Un refus vu par la conversation avant l'envoi (detail : `RefusDeChargement`). */
export const MODELE_REFUSE = "locaryn:modele-refuse";

const NIVEAUX: Record<CautionLevel, string> = {
  prudent: "Prudent",
  equilibre: "Équilibré",
  risque: "Risqué",
};

function depuisEvenement(payload: RefusDeChargement | string): RefusDeChargement {
  return typeof payload === "string"
    ? { message: payload, model: "", level: "equilibre", overridable: false }
    : payload;
}

/** Le refus affiché. Un second refus arrivé pendant qu'il est ouvert (envoi
 *  vérifié des deux côtés, autre appel) est la même tentative : ignoré. */
function useRefusAffiche() {
  const [refus, setRefus] = useState<RefusDeChargement | null>(null);
  const ouvert = useRef(false);
  const fermer = () => {
    ouvert.current = false;
    setRefus(null);
  };

  useEffect(() => {
    const montrer = (r: RefusDeChargement) => {
      if (ouvert.current) return;
      ouvert.current = true;
      setRefus(r);
    };
    const surRefusLocal = (ev: Event) => {
      const detail = (ev as CustomEvent<RefusDeChargement>).detail;
      if (detail) montrer(detail);
    };
    window.addEventListener(MODELE_REFUSE, surRefusLocal);
    const un = listen<RefusDeChargement | string>("locaryn://modele-trop-lourd", (ev) =>
      montrer(depuisEvenement(ev.payload)),
    ).catch((e: unknown) => {
      // Hors de Tauri (aperçu web), seul le refus vu par la conversation compte.
      console.debug("[ModeleTropLourd] écoute native indisponible :", e);
      return null;
    });
    return () => {
      window.removeEventListener(MODELE_REFUSE, surRefusLocal);
      void un.then((f) => f?.());
    };
  }, []);

  return { refus, fermer };
}

export function ModeleTropLourd({ onOpenSettings }: { onOpenSettings?: () => void }) {
  const { refus, fermer } = useRefusAffiche();
  const [charge, setCharge] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);

  function renoncer() {
    if (charge) return;
    setErreur(null);
    fermer();
    window.dispatchEvent(new Event(MODELE_ANNULE));
  }

  async function chargerQuandMeme() {
    if (!refus) return;
    setCharge(true);
    setErreur(null);
    try {
      // Résout une fois le moteur prêt à répondre : le message part ensuite,
      // sans passer par « Loading model ».
      await core.loadChatModel(refus.model, true);
      fermer();
      window.dispatchEvent(new CustomEvent(MODELE_FORCE, { detail: refus.model }));
    } catch (e) {
      setErreur(e instanceof Error ? e.message : String(e));
    } finally {
      setCharge(false);
    }
  }

  if (!refus) return null;
  return (
    <ModalShell
      label="Modèle trop lourd"
      role="alertdialog"
      onClose={renoncer}
      className="locaryn-card locaryn-trop-lourd"
    >
      <h3 className="locaryn-trop-lourd-titre">Modèle trop lourd pour être chargé</h3>
      <p className="locaryn-trop-lourd-texte">{refus.message}</p>
      <p className="locaryn-field-hint">
        Prudence actuelle : <strong>{NIVEAUX[refus.level]}</strong> (Réglages → Performance).
        Choisissez une version plus légère du modèle, un contexte plus court, ou un niveau de
        prudence moins strict.
      </p>
      {erreur && <p className="locaryn-field-error">{erreur}</p>}
      <div className="locaryn-trop-lourd-actions">
        {onOpenSettings && (
          <button
            type="button"
            className="locaryn-btn"
            disabled={charge}
            onClick={() => {
              renoncer();
              onOpenSettings();
            }}
          >
            Ouvrir les réglages
          </button>
        )}
        {refus.overridable && refus.model && (
          <button
            type="button"
            className="locaryn-btn"
            disabled={charge}
            aria-busy={charge}
            onClick={() => void chargerQuandMeme()}
          >
            {charge ? "Chargement…" : "Charger quand même"}
          </button>
        )}
        <button
          type="button"
          className="locaryn-btn locaryn-btn-primary"
          disabled={charge}
          onClick={renoncer}
        >
          Compris
        </button>
      </div>
    </ModalShell>
  );
}
