import { useCallback, useEffect, useRef, useState } from "react";

/**
 * La navigation du téléphone, adossée à l'historique du navigateur.
 *
 * Le bouton retour d'Android ne fait pas ce qu'on croit : il demande à la vue
 * web de reculer, et si elle n'a nulle part où reculer, il ferme
 * l'application. Une application d'une seule page ne pousse aucune entrée
 * d'historique — donc le retour quittait Locaryn depuis n'importe quel écran,
 * y compris depuis les réglages ouverts pour une seconde.
 *
 * La réponse n'est pas d'intercepter le geste, c'est de lui donner quelque
 * chose à faire : chaque écran ouvert pousse une entrée, chaque retour en
 * consomme une. L'historique devient la seule source de vérité — les boutons
 * « retour » de l'interface reculent par le même chemin que le geste système,
 * et il n'y a pas deux façons de reculer qui pourraient diverger.
 *
 * Arrivé à l'écran de départ, il ne reste rien à consommer : Android reprend
 * la main et ferme l'application. C'est ce qu'on attend à ce moment-là.
 */
export function useNavigation<T extends string>(racine: T) {
  const [ecran, setEcran] = useState<T>(racine);
  const racineRef = useRef<T>(racine);

  useEffect(() => {
    racineRef.current = racine;
  }, [racine]);

  useEffect(() => {
    // Initialise l'état de l'historique racine s'il est vide
    if (!window.history.state || typeof window.history.state.ecran === "undefined") {
      window.history.replaceState({ ecran: racineRef.current, profondeur: 0 }, "");
    }

    function auRetour(e: PopStateEvent) {
      const vers = (e.state as { ecran?: T } | null)?.ecran;
      setEcran(vers ?? racineRef.current);
    }
    window.addEventListener("popstate", auRetour);
    return () => window.removeEventListener("popstate", auRetour);
  }, []);

  /** Ouvrir un écran. Le retour y ramènera. */
  const aller = useCallback((vers: T) => {
    setEcran((actuel) => {
      // Rouvrir l'écran courant n'empile rien : sinon deux appuis sur le
      // même bouton demanderaient deux retours pour en sortir.
      if (vers === actuel) return actuel;
      if (vers === racineRef.current) {
        // Revenir à la racine, c'est vider la pile, pas l'allonger.
        window.history.go(-profondeur());
        return actuel;
      }
      window.history.pushState({ ecran: vers, profondeur: profondeur() + 1 }, "");
      return vers;
    });
  }, []);

  /** Reculer d'un cran, comme le ferait le geste système. */
  const revenir = useCallback(() => {
    window.history.back();
  }, []);

  /**
   * Remplacer l'écran sans empiler.
   *
   * Pour un changement qui n'est pas une navigation : se connecter, se
   * déconnecter. Reculer après cela devrait quitter, pas revenir sur un écran
   * qui ne veut plus rien dire.
   */
  const remplacer = useCallback((vers: T) => {
    racineRef.current = vers;
    window.history.replaceState({ ecran: vers, profondeur: 0 }, "");
    setEcran(vers);
  }, []);

  return { ecran, aller, revenir, remplacer };
}

/** À quelle profondeur de la pile on se trouve. */
function profondeur(): number {
  const etat = window.history.state as { profondeur?: number } | null;
  return etat?.profondeur ?? 0;
}
