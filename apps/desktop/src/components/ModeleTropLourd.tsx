// La fenêtre qui dit pourquoi un modèle n'a pas été chargé.
//
// La conversation qui démarre le moteur vérifie d'abord la prudence choisie
// (Réglages → Performance). Un refus dans le fil de la conversation passait
// inaperçu au milieu des messages : il mérite une fenêtre, avec la raison
// chiffrée et le chemin pour changer de niveau.

import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { type CautionLevel, core } from "../lib/core";
import { ModalShell } from "./ModalShell";

/** Le refus envoyé par le chargement automatique de la conversation. */
type Refus = {
  message: string;
  model: string;
  level: CautionLevel;
  overridable: boolean;
};

/** Le modèle a été chargé malgré la prudence : la conversation renvoie le message. */
export const MODELE_FORCE = "locaryn:modele-force";

const NIVEAUX: Record<CautionLevel, string> = {
  prudent: "Prudent",
  equilibre: "Équilibré",
  risque: "Risqué",
};

export function ModeleTropLourd({ onOpenSettings }: { onOpenSettings?: () => void }) {
  const [refus, setRefus] = useState<Refus | null>(null);
  const [charge, setCharge] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);

  useEffect(() => {
    const un = listen<Refus | string>("locaryn://modele-trop-lourd", (ev) => {
      setErreur(null);
      setRefus(
        typeof ev.payload === "string"
          ? { message: ev.payload, model: "", level: "equilibre", overridable: false }
          : ev.payload,
      );
    }).catch(() => null);
    return () => {
      void un.then((f) => f?.());
    };
  }, []);

  async function chargerQuandMeme() {
    if (!refus) return;
    setCharge(true);
    setErreur(null);
    try {
      await core.loadChatModel(refus.model, true);
      setRefus(null);
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
      onClose={() => setRefus(null)}
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
            onClick={() => {
              setRefus(null);
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
            onClick={() => void chargerQuandMeme()}
          >
            {charge ? "Chargement…" : "Charger quand même"}
          </button>
        )}
        <button
          type="button"
          className="locaryn-btn locaryn-btn-primary"
          onClick={() => setRefus(null)}
        >
          Compris
        </button>
      </div>
    </ModalShell>
  );
}
