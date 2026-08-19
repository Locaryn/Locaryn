import { useCallback, useEffect, useState } from "react";
import { MODEL_CATALOGUE, type ModelPullProgress, api } from "../lib/core";
import { Screen } from "./Screen";

type Props = { onBack: () => void };

/**
 * Le client web gère les modèles de chat via le serveur et les modèles audio
 * génériques de Locaryn. Les modèles d'image appartiennent exclusivement au
 * plugin image-gen et ne sont pas exposés ici.
 */
export function Models({ onBack }: Props) {
  const [voices, setVoices] = useState<string[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ModelPullProgress | null>(null);

  const reload = useCallback(async () => {
    try {
      const response = await api.listMediaModels("audio");
      setVoices(response.map((model) => model.name));
      setError(null);
    } catch (cause) {
      setError(String(cause));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function install(url: string, name: string) {
    setBusy(url);
    setError(null);
    setProgress({ downloaded: 0, total: null, percentage: null, message: null });
    try {
      await api.pullModel(url, setProgress);
      await reload();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
      setProgress(null);
    }
  }

  async function remove(name: string) {
    setBusy(name);
    setError(null);
    try {
      await api.removeModel(name);
      await reload();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(null);
    }
  }

  const catalogue = MODEL_CATALOGUE.filter((model) => model.kind === "audio");
  const installed = new Set(voices ?? []);

  return (
    <Screen title="Modèles" onBack={onBack}>
      {error && <p className="lo-error">{error}</p>}

      {progress && (
        <div className="lo-card" style={{ display: "block", marginBottom: 12 }}>
          <strong>Téléchargement en cours…</strong>
          <div className="lo-progress" aria-hidden="true">
            <div
              className="lo-progress-fill"
              style={progress.percentage == null ? undefined : { width: `${progress.percentage}%` }}
            />
          </div>
          <span className="lo-hint">
            {progress.message ??
              (progress.percentage == null ? "Préparation…" : `${progress.percentage} %`)}
          </span>
        </div>
      )}

      <section className="lo-section">
        <h2 className="lo-section-title">Voix & synthèse</h2>
        {voices === null && !error && <p className="lo-sub">Chargement…</p>}
        {voices?.length === 0 && <p className="lo-sub">Aucune voix installée.</p>}
        <ul className="lo-cards">
          {voices?.map((name) => (
            <li key={name} className="lo-card">
              <div className="lo-card-text">
                <span className="lo-card-title">{name}</span>
                <span className="lo-hint">Prêt pour la synthèse audio</span>
              </div>
              <button
                type="button"
                className="lo-btn-small"
                disabled={busy === name}
                onClick={() => void remove(name)}
              >
                {busy === name ? "Retrait…" : "Retirer"}
              </button>
            </li>
          ))}
        </ul>
      </section>

      <section className="lo-section">
        <h2 className="lo-section-title">Catalogue audio</h2>
        <p className="lo-hint">
          Les modèles sont installés sur le serveur et partagés avec vos appareils.
        </p>
        <ul className="lo-cards">
          {catalogue.map((model) => {
            const isInstalled = installed.has(model.name);
            const working = busy === model.url;
            return (
              <li key={model.url} className="lo-card">
                <div className="lo-card-text">
                  <span className="lo-card-title">{model.label}</span>
                  <span className="lo-hint">
                    {model.note} · {model.sizeGb} Go
                  </span>
                </div>
                {isInstalled ? (
                  <button type="button" className="lo-btn-small" disabled>
                    Installé
                  </button>
                ) : (
                  <button
                    type="button"
                    className="lo-btn-small lo-btn-small-on"
                    disabled={working || busy !== null}
                    onClick={() => void install(model.url, model.name)}
                  >
                    {working ? "Téléchargement…" : "Installer"}
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      </section>
    </Screen>
  );
}
