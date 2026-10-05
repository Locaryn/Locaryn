import { useCallback, useEffect, useState } from "react";
import { type ContextStatus, core } from "../lib/core";

/** Les événements après lesquels la fenêtre de contexte a pu changer. */
const EVENTS = [
  "locaryn:inference-config-changed",
  "locaryn:model-ejected",
  "locaryn:model-params-applied",
];

/** Le contexte est relu toutes les 5 s : le moteur peut démarrer, s'arrêter ou
 *  être relancé hors de cette fenêtre. */
const REFRESH_MS = 5000;

/**
 * L'état de la fenêtre de contexte, lu à un seul endroit.
 *
 * Le panneau du modèle, le profil du moteur et la jauge du chat affichaient
 * chacun une valeur à eux. Tous lisent maintenant ce hook : `configured` est ce
 * qu'on a réglé (le prochain chargement), `running` ce que le moteur a vraiment.
 */
export function useContextStatus(): {
  status: ContextStatus | null;
  /** Vrai quand le moteur tourne avec une autre fenêtre que celle réglée. */
  pending: boolean;
  refresh: () => Promise<void>;
} {
  const [status, setStatus] = useState<ContextStatus | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await core.contextStatus());
    } catch (e) {
      console.warn("contexte illisible :", e);
    }
  }, []);

  useEffect(() => {
    void refresh();
    for (const name of EVENTS) window.addEventListener(name, refresh);
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh();
    }, REFRESH_MS);
    return () => {
      for (const name of EVENTS) window.removeEventListener(name, refresh);
      window.clearInterval(timer);
    };
  }, [refresh]);

  const pending =
    status !== null && status.running !== null && status.running !== status.configured;
  return { status, pending, refresh };
}
