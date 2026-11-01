import { useEffect, useState } from "react";
import { core } from "./core";

/**
 * La fenêtre de contexte d'un modèle distant, lue dans le catalogue de son
 * fournisseur ; `null` tant qu'elle est inconnue ou pour un modèle local.
 *
 * Sans elle, la jauge de mémoire reprenait la fenêtre du moteur local resté
 * chargé — celle d'un autre modèle — ou disparaissait dès qu'il s'arrêtait.
 */
export function useCloudContext(model: string, remote: boolean): number | null {
  const [fenetre, setFenetre] = useState<number | null>(null);
  useEffect(() => {
    setFenetre(null);
    if (!remote || !model) return;
    let annule = false;
    void (async () => {
      try {
        const fournisseurs = await core.cloudProviders();
        const f = fournisseurs.find((p) => p.active_model === model);
        if (!f) return;
        const modeles = await core.cloudProviderModels(f.id, false);
        const m = modeles.find((x) => x.id === model);
        if (!annule && m && m.context_length > 0) setFenetre(m.context_length);
      } catch (e) {
        console.warn(`[Mémoire] fenêtre de ${model} illisible :`, e);
      }
    })();
    return () => {
      annule = true;
    };
  }, [model, remote]);
  return fenetre;
}
