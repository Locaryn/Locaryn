import { useCallback, useEffect, useState } from "react";
import {
  type CatalogueModel,
  MODEL_CATALOGUE,
  type MediaModel,
  type ModelPullProgress,
  api,
} from "../lib/core";
import { Screen } from "./Screen";

type Props = { onBack: () => void };

/**
 * Les modèles du serveur — installés d'abord, puis le catalogue de ceux qu'on
 * peut installer, comme le marketplace des extensions. Le navigateur n'en
 * héberge aucun : il regarde ce qui tourne sur la machine d'en face, et ne
 * fait que désigner lequel télécharger.
 */
export function Models({ onBack }: Props) {
  const [images, setImages] = useState<MediaModel[] | null>(null);
  const [voices, setVoices] = useState<MediaModel[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** L'avancement du téléchargement en cours, pour la barre du catalogue. */
  const [progress, setProgress] = useState<ModelPullProgress | null>(null);

  const reload = useCallback(async () => {
    try {
      const [i, v] = await Promise.all([
        api.listMediaModels("image"),
        api.listMediaModels("audio"),
      ]);
      setImages(i);
      setVoices(v);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function install(c: CatalogueModel) {
    setBusy(c.url);
    setError(null);
    setProgress({ downloaded: 0, total: null, percentage: null, message: null });
    try {
      await api.pullModel(c.url, setProgress);
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
      setProgress(null);
    }
  }

  /** Retirer un modèle installé : ses fichiers disparaissent du serveur. */
  async function remove(name: string) {
    setBusy(name);
    setError(null);
    try {
      await api.removeModel(name);
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  const installed = new Set([
    ...(images ?? []).map((m) => m.name),
    ...(voices ?? []).map((m) => m.name),
  ]);

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
              <div className="lo-card-actions">
                <button
                  type="button"
                  className="lo-btn-small"
                  disabled={busy === m.name}
                  onClick={() => void remove(m.name)}
                >
                  {busy === m.name ? "Retrait…" : "Retirer"}
                </button>
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
              <div className="lo-card-actions">
                <button
                  type="button"
                  className="lo-btn-small"
                  disabled={busy === m.name}
                  onClick={() => void remove(m.name)}
                >
                  {busy === m.name ? "Retrait…" : "Retirer"}
                </button>
              </div>
            </li>
          ))}
        </ul>
      </section>

      <section className="lo-section">
        <h2 className="lo-section-title">Catalogue</h2>
        <p className="lo-hint">
          Ils s'installent sur le serveur et valent pour tous ses appareils. Un modèle d'image
          télécharge aussi ce qui lui manque (VAE, encodeur) pour produire dès l'installation.
        </p>
        <ul className="lo-cards">
          {MODEL_CATALOGUE.map((c) => {
            const on = installed.has(c.name);
            const working = busy === c.url;
            return (
              <li key={c.url} className="lo-card">
                <div className="lo-card-text">
                  <span className="lo-card-title">{c.label}</span>
                  <span className="lo-hint">
                    {c.note} · {c.sizeGb} Go
                  </span>
                </div>
                <div className="lo-card-actions">
                  {on ? (
                    <button type="button" className="lo-btn-small" disabled>
                      Installé
                    </button>
                  ) : (
                    <button
                      type="button"
                      className="lo-btn-small lo-btn-small-on"
                      disabled={working}
                      onClick={() => void install(c)}
                    >
                      {working ? "Téléchargement…" : "Installer"}
                    </button>
                  )}
                </div>
                {working && progress && (
                  <>
                    {/* Le pourcentage vit dans le texte et dans la ligne des
                        tailles : la barre est décorative, comme celle de la
                        mise à jour. */}
                    <div
                      className={`lo-progress${progress.percentage == null ? " lo-progress-indeterminate" : ""}`}
                      aria-hidden="true"
                    >
                      <div
                        className="lo-progress-fill"
                        style={
                          progress.percentage == null
                            ? undefined
                            : { width: `${progress.percentage}%` }
                        }
                      />
                    </div>
                    <p className="lo-hint">
                      {progress.message ??
                        (progress.percentage != null
                          ? `Téléchargement… ${progress.percentage} %`
                          : "Téléchargement…")}
                      {progress.total != null && progress.total > 0
                        ? ` · ${Math.round(progress.downloaded / (1024 * 1024))} Mo sur ${Math.round(progress.total / (1024 * 1024))} Mo`
                        : ""}
                    </p>
                  </>
                )}
              </li>
            );
          })}
        </ul>
      </section>
    </Screen>
  );
}
