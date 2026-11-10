// Les questions que l'application pose : confirmation, message, saisie.
//
// Elles passaient par les boîtes du système (`confirm`, `alert`, `prompt`) :
// une fenêtre Windows grise au milieu d'une interface sombre, avec ses propres
// boutons et sa propre police. Elles sont dessinées ici, avec les mêmes
// cartes et boutons que le reste.

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import {
  type DemandeDialogue,
  dialogueCourant,
  repondreDialogue,
  suivreDialogue,
} from "../lib/dialog";
import { ModalShell } from "./ModalShell";

function Corps({ demande }: { demande: DemandeDialogue }) {
  const [valeur, setValeur] = useState(demande.genre === "saisie" ? demande.defaut : "");
  const principal = useRef<HTMLButtonElement>(null);
  const champ = useRef<HTMLInputElement>(null);

  // Le focus va au champ pour une saisie, sinon au bouton sans risque :
  // Entrée ne doit jamais confirmer une suppression par réflexe.
  useEffect(() => {
    if (demande.genre === "saisie") {
      champ.current?.focus();
      champ.current?.select();
    } else {
      principal.current?.focus();
    }
  }, [demande.genre]);

  const refuser = () => repondreDialogue(demande, demande.genre === "saisie" ? null : false);
  const accepter = () => repondreDialogue(demande, demande.genre === "saisie" ? valeur : true);

  return (
    <ModalShell
      onClose={refuser}
      label={demande.titre}
      role={demande.genre === "saisie" ? "dialog" : "alertdialog"}
      className="locaryn-card locaryn-app-dialog"
      overlayClassName="locaryn-modal-overlay locaryn-app-dialog-voile"
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          accepter();
        }}
      >
        <h3 className="locaryn-app-dialog-titre">{demande.titre}</h3>
        {demande.message && <p className="locaryn-app-dialog-message">{demande.message}</p>}
        {demande.genre === "saisie" && (
          <input
            ref={champ}
            className="locaryn-input"
            value={valeur}
            aria-label={demande.titre}
            onChange={(e) => setValeur(e.target.value)}
          />
        )}
        <div className="locaryn-app-dialog-actions">
          {demande.genre === "message" ? (
            <button ref={principal} type="submit" className="locaryn-btn-primary">
              Compris
            </button>
          ) : demande.genre === "saisie" ? (
            <>
              <button type="button" className="locaryn-btn-ghost" onClick={refuser}>
                Annuler
              </button>
              <button type="submit" className="locaryn-btn-primary">
                Valider
              </button>
            </>
          ) : (
            <>
              <button ref={principal} type="button" className="locaryn-btn-ghost" onClick={refuser}>
                Annuler
              </button>
              <button type="submit" className="locaryn-btn-primary">
                Confirmer
              </button>
            </>
          )}
        </div>
      </form>
    </ModalShell>
  );
}

/** À monter une fois, à la racine : affiche la question en cours. */
export function AppDialogs() {
  const demande = useSyncExternalStore(suivreDialogue, dialogueCourant);
  return demande ? <Corps key={demande.id} demande={demande} /> : null;
}
