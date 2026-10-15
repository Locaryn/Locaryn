import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { type EtatDecisions, core, isTauri } from "../lib/core";

type Progres = { etape: string; detail: string; fini: boolean; erreur: string | null };

/**
 * Le modèle de décision local que partagent les morphs de contrôle (bureau,
 * navigateur) : « quel bouton cliquer », « l'objectif est-il atteint », en
 * une centaine de millisecondes, sans occuper le modèle de conversation.
 * L'application l'installe d'elle-même quand un de ces morphs arrive ; ici on
 * voit où il en est, et on peut l'installer ou le réparer.
 */
export function DecisionSettings() {
  const [etat, setEtat] = useState<EtatDecisions | null>(null);
  const [progres, setProgres] = useState<Progres | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);

  useEffect(() => {
    core
      .decisionsEtat()
      .then(setEtat)
      .catch(() => {
        // Sans état lisible, la carte propose simplement l'installation.
      });
    if (!isTauri) return;
    const un = listen<Progres>("locaryn://laya-progres", (ev) => {
      setProgres(ev.payload.fini ? null : ev.payload);
      if (ev.payload.fini) {
        setErreur(ev.payload.erreur);
        void core.decisionsEtat().then(setEtat);
      }
    }).catch(() => null);
    return () => {
      void un.then((f) => f?.());
    };
  }, []);

  async function installer() {
    setErreur(null);
    setProgres({ etape: "debut", detail: "Préparation…", fini: false, erreur: null });
    try {
      setEtat(await core.decisionsInstaller());
    } catch (e) {
      setErreur(e instanceof Error ? e.message : String(e));
    } finally {
      setProgres(null);
    }
  }

  const enCours = progres !== null || etat?.en_cours === true;
  return (
    <div className="locaryn-field">
      <span className="locaryn-field-label">Décisions rapides des morphs</span>
      <p className="locaryn-field-hint">
        Un petit modèle local aide le contrôle du bureau et du navigateur à choisir où cliquer et à
        vérifier le résultat, en une fraction de seconde, sans occuper le modèle de conversation. Il
        s'installe tout seul avec ces morphs (environ 1,3 Go, sur le disque de vos modèles).
      </p>
      <div className="locaryn-decisions-ligne">
        <span className={`locaryn-decisions-etat${etat?.installe ? " is-ok" : ""}`}>
          {enCours
            ? (progres?.detail ?? "Installation en cours…")
            : etat?.installe
              ? `Installé${etat.appareil === "cuda" ? " (carte NVIDIA)" : " (processeur)"}`
              : "Non installé"}
        </span>
        <button
          type="button"
          className="locaryn-btn"
          disabled={enCours}
          onClick={() => void installer()}
        >
          {etat?.installe ? "Réinstaller" : "Installer"}
        </button>
      </div>
      {erreur && <p className="locaryn-field-error">{erreur}</p>}
    </div>
  );
}
