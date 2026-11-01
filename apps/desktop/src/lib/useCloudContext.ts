import { useEffect, useState } from "react";
import { core } from "./core";

/** Ce que l'hôte sait du modèle distant actif. */
export interface ModeleDistant {
  /** Le fournisseur qui le sert (identifiant de son catalogue). */
  fournisseur: string | null;
  /** Sa fenêtre de contexte, publiée par le catalogue. */
  fenetre: number | null;
}

const RIEN: ModeleDistant = { fournisseur: null, fenetre: null };

/**
 * Le fournisseur et la fenêtre de contexte d'un modèle distant, lus dans son
 * catalogue ; rien pour un modèle local.
 *
 * Sans la fenêtre, la jauge de mémoire reprenait celle du moteur local resté
 * chargé — celle d'un autre modèle — ou disparaissait dès qu'il s'arrêtait.
 */
export function useCloudContext(model: string, remote: boolean): ModeleDistant {
  const [info, setInfo] = useState<ModeleDistant>(RIEN);
  useEffect(() => {
    setInfo(RIEN);
    if (!remote || !model) return;
    let annule = false;
    void (async () => {
      try {
        const fournisseurs = await core.cloudProviders();
        const f = fournisseurs.find((p) => p.active_model === model);
        if (!f) return;
        if (!annule) setInfo({ fournisseur: f.id, fenetre: null });
        const modeles = await core.cloudProviderModels(f.id, false);
        const m = modeles.find((x) => x.id === model);
        if (!annule && m && m.context_length > 0) {
          setInfo({ fournisseur: f.id, fenetre: m.context_length });
        }
      } catch (e) {
        console.warn(`[Modèle] catalogue de ${model} illisible :`, e);
      }
    })();
    return () => {
      annule = true;
    };
  }, [model, remote]);
  return info;
}
