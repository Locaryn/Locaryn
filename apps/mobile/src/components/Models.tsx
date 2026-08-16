import { useEffect, useState } from "react";
import { type MediaModel, api } from "../lib/core";
import { Screen } from "./Screen";

type Props = { onBack: () => void };

/**
 * Les modèles installés sur le serveur.
 *
 * Le téléphone n'en héberge aucun : il les regarde. Ce qui compte ici est de
 * savoir lesquels peuvent réellement produire quelque chose — un poids de
 * diffusion sans son encodeur occupe de la place et ne rendra jamais d'image,
 * et il vaut mieux le lire ici que le découvrir au moment de générer.
 */
export function Models({ onBack }: Props) {
  const [images, setImages] = useState<MediaModel[] | null>(null);
  const [voices, setVoices] = useState<MediaModel[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [i, v] = await Promise.all([
          api.listMediaModels("image"),
          api.listMediaModels("audio"),
        ]);
        if (cancelled) return;
        setImages(i);
        setVoices(v);
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <Screen title="Modèles" onBack={onBack}>
      {error && <p className="lo-error">{error}</p>}

      <section className="lo-section">
        <h2 className="lo-section-title">Images</h2>
        {images === null && !error && <p className="lo-sub">Chargement…</p>}
        {images?.length === 0 && <p className="lo-sub">Aucun modèle d'image installé.</p>}
        <ul className="lo-cards">
          {images?.map((m) => (
            <li key={m.name} className="lo-card">
              <div className="lo-card-text">
                <span className="lo-card-title">{m.name}</span>
                {m.ready ? (
                  <span className="lo-hint">Prêt</span>
                ) : (
                  <span className="lo-hint">Incomplet — il manque {m.missing.join(" et ")}</span>
                )}
              </div>
            </li>
          ))}
        </ul>
      </section>

      <section className="lo-section">
        <h2 className="lo-section-title">Voix</h2>
        {voices === null && !error && <p className="lo-sub">Chargement…</p>}
        {voices?.length === 0 && <p className="lo-sub">Aucune voix installée.</p>}
        <ul className="lo-cards">
          {voices?.map((m) => (
            <li key={m.name} className="lo-card">
              <div className="lo-card-text">
                <span className="lo-card-title">{m.name}</span>
              </div>
            </li>
          ))}
        </ul>
      </section>

      <p className="lo-hint">
        L'installation de nouveaux modèles se fait sur l'ordinateur, là où ils sont téléchargés et
        où ils tournent.
      </p>
    </Screen>
  );
}
