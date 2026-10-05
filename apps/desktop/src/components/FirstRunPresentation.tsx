import { Presentation } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { PRESENTATION_BUREAU, PRESENTATION_KEY, PRESENTATION_REPLAY } from "../lib/presentation";

/**
 * La présentation au premier lancement, et à la demande depuis « À propos ».
 */
export function FirstRunPresentation() {
  const [rejouer, setRejouer] = useState(false);

  useEffect(() => {
    const surDemande = () => setRejouer(true);
    window.addEventListener(PRESENTATION_REPLAY, surDemande);
    return () => window.removeEventListener(PRESENTATION_REPLAY, surDemande);
  }, []);

  return (
    <Presentation
      steps={PRESENTATION_BUREAU}
      storageKey={PRESENTATION_KEY}
      forceOpen={rejouer}
      onClose={() => setRejouer(false)}
    />
  );
}
