import { useCallback, useEffect, useState } from "react";
import { type CatalogueEntry, type PhoneExtension, api } from "../lib/core";
import { Screen } from "./Screen";

type Props = {
  onBack: () => void;
  /** Une extension a bougé : les capacités changent, donc l'interface aussi. */
  onChanged: () => void;
};

/**
 * Les extensions du serveur — les mêmes que sur le téléphone. Elles
 * s'installent sur la machine d'en face ; le navigateur ne fait que désigner
 * laquelle. Le catalogue vient du serveur : aucune liste n'est tenue ici.
 */
export function Extensions({ onBack, onChanged }: Props) {
  const [installed, setInstalled] = useState<PhoneExtension[] | null>(null);
  const [catalog, setCatalog] = useState<CatalogueEntry[] | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setInstalled(await api.listExtensions());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    let cancelled = false;
    const t = setTimeout(() => {
      api
        .extensionCatalog(query)
        .then((entries) => {
          if (!cancelled) setCatalog(entries);
        })
        .catch((e) => {
          if (!cancelled) setError(String(e));
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [query]);

  async function act(key: string, run: () => Promise<unknown>) {
    setBusy(key);
    setError(null);
    try {
      await run();
      await reload();
      onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  const installedNames = new Set((installed ?? []).map((e) => e.name));
  const offers = (catalog ?? []).filter((c) => !c.installed && !installedNames.has(c.name));

  return (
    <Screen title="Extensions" onBack={onBack}>
      <p className="lo-hint">
        Elles s'installent sur le serveur et valent pour tous ses appareils. Ce qu'elles apportent
        apparaît ensuite dans le Studio et dans les réponses du modèle.
      </p>
      {error && <p className="lo-error">{error}</p>}

      <section className="lo-section">
        <h2 className="lo-section-title">Installées</h2>
        {installed === null && !error && <p className="lo-sub">Chargement…</p>}
        {installed?.length === 0 && <p className="lo-sub">Aucune extension installée.</p>}
        <ul className="lo-cards">
          {installed?.map((ext) => {
            const working = busy === ext.name;
            return (
              <li key={ext.name} className="lo-card">
                <div className="lo-card-text">
                  <span className="lo-card-title">{ext.display_name || ext.name}</span>
                  {ext.description && <span className="lo-hint">{ext.description}</span>}
                  {!ext.enabled && <span className="lo-tag">désactivée</span>}
                </div>
                <div className="lo-card-actions">
                  <button
                    type="button"
                    className="lo-btn-small"
                    disabled={working}
                    onClick={() =>
                      act(ext.name, () => api.setExtensionEnabled(ext.name, !ext.enabled))
                    }
                  >
                    {ext.enabled ? "Désactiver" : "Activer"}
                  </button>
                  <button
                    type="button"
                    className="lo-btn-small"
                    disabled={working}
                    onClick={() => act(ext.name, () => api.removeExtension(ext.name))}
                  >
                    Retirer
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      </section>

      <section className="lo-section">
        <h2 className="lo-section-title">Catalogue</h2>
        <input
          type="search"
          className="lo-input lo-offre-recherche"
          placeholder="Rechercher une extension"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        {catalog === null && !error && <p className="lo-sub">Lecture du catalogue…</p>}
        {catalog !== null && offers.length === 0 && (
          <p className="lo-sub">Aucune extension ne correspond.</p>
        )}
        <ul className="lo-cards">
          {offers.map((c) => {
            const working = busy === c.install_source;
            return (
              <li key={c.id} className="lo-card">
                <div className="lo-card-text">
                  <span className="lo-card-title">{c.display_name}</span>
                  {c.description && <span className="lo-hint">{c.description}</span>}
                </div>
                <div className="lo-card-actions">
                  <button
                    type="button"
                    className="lo-btn-small lo-btn-small-on"
                    disabled={working || busy !== null}
                    onClick={() =>
                      act(c.install_source, () => api.installExtension(c.install_source))
                    }
                  >
                    {working ? "Installation…" : "Installer"}
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      </section>
    </Screen>
  );
}
