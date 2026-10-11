import { LoProgress } from "@locaryn/ui-core";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  type CatalogueMarketplace,
  type MarketplaceFamily,
  type MarketplaceVariant,
  type ModelPullProgress,
  api,
} from "../lib/core";
import { Screen } from "./Screen";

type Props = { onBack: () => void };

/** La plus légère d'abord : c'est elle qui a le plus de chances de tenir. */
function variantesTriees(f: MarketplaceFamily): MarketplaceVariant[] {
  return [...f.variants].sort((x, y) => (x.storageGb ?? 0) - (y.storageGb ?? 0));
}

/**
 * Les modèles du serveur. Le catalogue vient de `/v1/marketplace` : les
 * modèles de conversation du socle et ceux des morphs actifs. Aucune catégorie
 * ni aucun modèle spécialisé n'est tenu ici : sans le morph, ils n'existent pas.
 */
export function Models({ onBack }: Props) {
  const [chat, setChat] = useState<string[] | null>(null);
  const [audio, setAudio] = useState<string[]>([]);
  const [catalogue, setCatalogue] = useState<CatalogueMarketplace | null>(null);
  const [categorie, setCategorie] = useState("all");
  const [recherche, setRecherche] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ModelPullProgress | null>(null);

  const reload = useCallback(async () => {
    try {
      const [c, a] = await Promise.all([
        api.listModels().catch(() => [] as string[]),
        api.listMediaModels("audio").catch(() => []),
      ]);
      setChat(c);
      setAudio(a.map((m) => m.name));
      setError(null);
    } catch (cause) {
      setError(String(cause));
    }
  }, []);

  useEffect(() => {
    void reload();
    api
      .marketplace()
      .then(setCatalogue)
      .catch((cause) => setError(String(cause)));
  }, [reload]);

  async function install(f: MarketplaceFamily, v: MarketplaceVariant) {
    setBusy(v.tag);
    setError(null);
    setProgress({ downloaded: 0, total: null, percentage: null, message: null });
    try {
      await api.pullModel(v.tag, setProgress, v.downloads);
      await reload();
    } catch (cause) {
      setError(`${f.name} ${v.size} : ${String(cause)}`);
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

  const installes = useMemo(() => [...(chat ?? []), ...audio], [chat, audio]);
  const installedSet = useMemo(() => new Set(installes), [installes]);

  const familles = useMemo(() => {
    if (!catalogue) return [];
    const cat = catalogue.categories.find((c) => c.id === categorie);
    const q = recherche.trim().toLowerCase();
    return catalogue.families.filter((f) => {
      if (cat && !cat.matches.some((m) => (f.marketplaceCapabilities ?? []).includes(m))) {
        return false;
      }
      if (!q) return true;
      return `${f.name} ${f.brand ?? ""} ${f.description ?? ""}`.toLowerCase().includes(q);
    });
  }, [catalogue, categorie, recherche]);

  return (
    <Screen title="Modèles" onBack={onBack}>
      {error && <p className="lo-error">{error}</p>}

      {progress && (
        <div className="lo-card" style={{ display: "block", marginBottom: 12 }}>
          <strong>Téléchargement en cours…</strong>
          <LoProgress
            value={progress.percentage == null ? null : progress.percentage / 100}
            on="surface"
            label="Téléchargement"
          />
          <span className="lo-hint">
            {progress.message ??
              (progress.percentage == null ? "Préparation…" : `${progress.percentage} %`)}
          </span>
        </div>
      )}

      <section className="lo-section">
        <h2 className="lo-section-title">Installés</h2>
        {chat === null && !error && <p className="lo-sub">Chargement…</p>}
        {chat !== null && installes.length === 0 && (
          <p className="lo-sub">Aucun modèle sur le serveur.</p>
        )}
        <ul className="lo-cards">
          {installes.map((name) => (
            <li key={name} className="lo-card">
              <div className="lo-card-text">
                <span className="lo-card-title">{name}</span>
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
        <h2 className="lo-section-title">Marketplace</h2>
        <p className="lo-hint">
          Les modèles s'installent sur le serveur. Un morph actif y ajoute ses catégories.
        </p>
        <input
          type="search"
          className="lo-input lo-offre-recherche"
          placeholder="Rechercher un modèle"
          value={recherche}
          onChange={(e) => setRecherche(e.target.value)}
        />
        <div className="lo-chips">
          {[{ id: "all", label: "Tous" }, ...(catalogue?.categories ?? [])].map((c) => (
            <button
              key={c.id}
              type="button"
              className={`lo-chip ${categorie === c.id ? "lo-chip-on" : ""}`}
              onClick={() => setCategorie(c.id)}
            >
              {c.label}
            </button>
          ))}
        </div>
        {!catalogue && !error && <p className="lo-sub">Lecture du Marketplace…</p>}
        {catalogue && familles.length === 0 && (
          <p className="lo-sub">Aucun modèle ne correspond.</p>
        )}
        <ul className="lo-cards">
          {familles.map((f) => (
            <li key={f.id} className="lo-card lo-offre">
              <div className="lo-card-text">
                <span className="lo-card-title">{f.name}</span>
                <span className="lo-hint">
                  {[f.brand, f.marketplaceOwner && `apporté par ${f.marketplaceOwner}`]
                    .filter(Boolean)
                    .join(" · ")}
                </span>
                {f.description && <span className="lo-hint">{f.description}</span>}
              </div>
              <div className="lo-offre-variantes">
                {variantesTriees(f).map((v) => {
                  const installe = installedSet.has(v.tag);
                  return (
                    <button
                      key={v.tag}
                      type="button"
                      className="lo-offre-variante"
                      disabled={installe || busy !== null}
                      onClick={() => void install(f, v)}
                    >
                      <strong>{v.size}</strong>
                      <span>
                        {installe
                          ? "Installé"
                          : busy === v.tag
                            ? "Téléchargement…"
                            : v.storageGb
                              ? `${v.storageGb} Go`
                              : "Obtenir"}
                      </span>
                    </button>
                  );
                })}
              </div>
            </li>
          ))}
        </ul>
      </section>
    </Screen>
  );
}
