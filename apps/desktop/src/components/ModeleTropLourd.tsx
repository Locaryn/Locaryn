// La fenêtre qui dit pourquoi un modèle n'a pas été chargé.
//
// La conversation qui démarre le moteur vérifie d'abord la prudence choisie
// (Réglages → Performance). Un refus dans le fil de la conversation passait
// inaperçu au milieu des messages : il mérite une fenêtre, avec la raison
// chiffrée et le chemin pour changer de niveau.

import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { ModalShell } from "./ModalShell";

export function ModeleTropLourd({ onOpenSettings }: { onOpenSettings?: () => void }) {
  const [raison, setRaison] = useState<string | null>(null);

  useEffect(() => {
    const un = listen<string>("locaryn://modele-trop-lourd", (ev) => setRaison(ev.payload)).catch(
      () => null,
    );
    return () => {
      void un.then((f) => f?.());
    };
  }, []);

  if (!raison) return null;
  return (
    <ModalShell
      label="Modèle trop lourd"
      role="alertdialog"
      onClose={() => setRaison(null)}
      className="locaryn-card locaryn-trop-lourd"
    >
      <h3 className="locaryn-trop-lourd-titre">Modèle trop lourd pour être chargé</h3>
      <p className="locaryn-trop-lourd-texte">{raison}</p>
      <p className="locaryn-field-hint">
        Le chargement ne respecte pas la prudence choisie dans Réglages → Performance. Choisissez
        une version plus légère du modèle, un contexte plus court, ou un niveau de prudence moins
        strict.
      </p>
      <div className="locaryn-trop-lourd-actions">
        {onOpenSettings && (
          <button
            type="button"
            className="locaryn-btn"
            onClick={() => {
              setRaison(null);
              onOpenSettings();
            }}
          >
            Ouvrir les réglages
          </button>
        )}
        <button type="button" className="locaryn-btn locaryn-btn-primary" onClick={() => setRaison(null)}>
          Compris
        </button>
      </div>
    </ModalShell>
  );
}
