import { Icon } from "@locaryn/ui-core";
import { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { SpeedBadge, findMetric } from "../components/SpeedBadge";
import {
  BadgeEnLigne,
  CloudProviderScreen,
  CloudProviderTile,
  type ModeleEnLigne,
  badgesEnLigne,
  useCloudProviders,
  useModelesEnLigne,
} from "../components/cloud/CloudProviderFolder";
import { type InstalledExtension, type ModelMetric, type StoredWeight, core } from "../lib/core";
import { claimantOf, loadExtensionMarketplaces } from "../lib/extensionMarketplace";
import { focusPremiereEntree, navigationMenu, useMenuUnique } from "../lib/menuUnique";
import { type NomDeModele, decrireModele } from "../lib/modelName";
import { classifyModel, nsfwReason } from "../lib/modelSafety";
import { VUE_RECHOISIE } from "../lib/vues";

import { confirmer, informer } from "../lib/dialog";
type Props = {
  installedModels: string[];
  onSelectModelForChat: (modelTag: string) => void;
  onDeleteModel?: (modelTag: string) => Promise<void> | void;
  onOpenMarketplace?: () => void;
  /** Extensions actives : chacune revendique ses propres poids par catalogue. */
  extensions?: InstalledExtension[];
};

/** Une taille lisible, pour un écran dont le sujet est la place occupée. */
function humanSize(bytes: number): string {
  if (bytes <= 0) return "";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(gb >= 10 ? 0 : 1)} Go`;
  return `${Math.max(1, Math.round(bytes / 1024 ** 2))} Mo`;
}

/** Au-delà, les variantes d'un nom se résument en « +N ». */
const VARIANTES_MAX = 5;

/** Les étiquettes d'une carte : quantification, variantes du nom, format. */
function etiquettesDuNom(nom: NomDeModele): string[] {
  const visibles = nom.variantes.slice(0, VARIANTES_MAX);
  const reste = nom.variantes.length - visibles.length;
  // « MODEL » : un dossier sans extension de fichier — rien à dire du format.
  const format = nom.format === "MODEL" ? null : nom.format;
  return [nom.quantification, ...visibles, reste > 0 ? `+${reste}` : null, format].filter(
    (t): t is string => !!t,
  );
}

function checkIsModelDebrided(entry: StoredEntry, debridedList: string[]): boolean {
  if (!debridedList || debridedList.length === 0) return false;
  const tagLow = entry.tag.toLowerCase().trim();
  const titleLow = entry.title.toLowerCase().trim();
  const pathLow = (entry.path || "").toLowerCase().trim();
  const fileLow = (entry.path || entry.tag).split(/[/\\]/).pop()?.toLowerCase().trim() ?? "";

  return debridedList.some((raw) => {
    const d = raw.toLowerCase().trim();
    if (!d) return false;
    const dFile = d.split(/[/\\]/).pop()?.toLowerCase().trim() ?? d;
    return (
      tagLow === d ||
      tagLow.includes(d) ||
      d.includes(tagLow) ||
      titleLow === d ||
      titleLow.includes(d) ||
      d.includes(titleLow) ||
      pathLow === d ||
      pathLow.includes(d) ||
      d.includes(pathLow) ||
      (fileLow.length > 0 &&
        dFile.length > 0 &&
        (fileLow === dFile || fileLow.includes(dFile) || dFile.includes(fileLow)))
    );
  });
}

/** D'où vient un modèle stocké, et ce qu'on peut en faire.
 *
 *  `chat` : le moteur de conversation le charge. `extension` : une extension
 *  s'en sert — l'hôte ne sait pas à quoi. `incompatible` : un dépôt que rien
 *  n'exécute ici, gardé visible pour pouvoir libérer la place. */
type EntryKind = "chat" | "extension" | "incompatible";

type StoredEntry = {
  /** L'identifiant que les commandes de suppression et d'ouverture attendent. */
  tag: string;
  kind: EntryKind;
  title: string;
  /** Ligne d'en-tête : qui charge ce modèle. */
  brand: string;
  /** Le nom sous lequel ce modèle se filtre. Une puce par valeur présente :
   *  la liste se remplit d'elle-même quand une extension arrive, sans que le
   *  socle ait à connaître les catégories qui existent. */
  group: string;
  tags: string[];
  path: string;
  hint?: string;
};

export function InstalledModelsView({
  installedModels,
  onSelectModelForChat,
  onDeleteModel,
  onOpenMarketplace,
  extensions = [],
}: Props) {
  const [query, setQuery] = useState("");
  const [riskFilter, setRiskFilter] = useState<"all" | "safe" | "uncensored" | "nsfw">("all");
  /* Les catalogues distants apportés par les morphs : un dossier chacun, à la
     place d'une carte de modèle. Ouvert, il remplace cet écran — c'est bien
     une page, pas un panneau qui se glisse à côté. */
  const { providers: cloudProviders, reload: reloadCloudProviders } = useCloudProviders();
  const [relecture, setRelecture] = useState(0);
  const enLigne = useModelesEnLigne(cloudProviders, true, relecture);
  // Ceux qu'un moteur de la machine a déjà téléchargés : ils occupent le
  // disque comme les autres, et se suppriment d'ici.
  const telecharges = enLigne.filter((m) => m.model.installed === true);
  const [occupe, setOccupe] = useState<string | null>(null);

  async function utiliserEnLigne(m: ModeleEnLigne) {
    setOccupe(m.model.id);
    try {
      await core.cloudProviderSelect(m.provider.id, m.model.id);
      reloadCloudProviders();
    } catch (e) {
      void informer(String(e).replace(/^Error:\s*/, ""), "Sélection impossible");
    } finally {
      setOccupe(null);
    }
  }

  async function supprimerEnLigne(m: ModeleEnLigne) {
    const oui = await confirmer(
      `${m.model.name} sera retiré du disque. Vous pourrez le télécharger à nouveau.`,
      "Supprimer ce modèle ?",
    );
    if (!oui) return;
    setOccupe(m.model.id);
    try {
      await core.cloudProviderDeleteModel(m.provider.id, m.model.id);
      setRelecture((n) => n + 1);
    } catch (e) {
      void informer(String(e).replace(/^Error:\s*/, ""), "Suppression impossible");
    } finally {
      setOccupe(null);
    }
  }
  const [openFolder, setOpenFolder] = useState<string | null>(null);
  // Recliquer « Modèles » dans la barre latérale referme le dossier ouvert.
  useEffect(() => {
    const surRechoix = (e: Event) => {
      if ((e as CustomEvent<string>).detail === "installed") setOpenFolder(null);
    };
    window.addEventListener(VUE_RECHOISIE, surRechoix);
    return () => window.removeEventListener(VUE_RECHOISIE, surRechoix);
  }, []);
  const [groupFilter, setGroupFilter] = useState<string | null>(null);
  const [activatingModel, setActivatingModel] = useState<string | null>(null);
  const [modelsDir, setModelsDir] = useState("");
  const [metrics, setMetrics] = useState<ModelMetric[]>([]);
  const [incompatibleModels, setIncompatibleModels] = useState<string[]>([]);
  // Les poids que la conversation ne charge pas. Ils étaient simplement
  // absents de cet écran : installés, occupant des gigaoctets, invisibles.
  const [otherWeights, setOtherWeights] = useState<StoredWeight[]>([]);
  const [tailles, setTailles] = useState<Record<string, number>>({});
  const [weightOwners, setWeightOwners] = useState<Record<string, string>>({});
  // The backend already groups shards, but deliberately keeps separate
  // quantisations/variants. Do not collapse by directory here or Q4 and Q8
  // from the same HuggingFace repository would appear as one model again.
  const dedupedModels = useMemo(
    () => Array.from(new Set(installedModels)).sort((a, b) => a.localeCompare(b)),
    [installedModels],
  );

  const {
    menu: contextMenu,
    ouvrir: ouvrirMenu,
    fermer: fermerMenu,
  } = useMenuUnique<{ x: number; y: number; entry: StoredEntry }>();
  const setContextMenu = (valeur: null) => {
    if (valeur === null) fermerMenu();
  };
  const [noticeMessage, setNoticeMessage] = useState<string | null>(null);
  const [debridedModels, setDebridedModels] = useState<string[]>([]);

  useEffect(() => {
    core
      .appInfo()
      .then((info) => setModelsDir(info.models_dir || `${info.data_dir}/models`))
      .catch(() => {});
    void core
      .listModelMetrics()
      .then(setMetrics)
      .catch(() => setMetrics([]));
    void core
      .listIncompatibleModels()
      .then(setIncompatibleModels)
      .catch(() => setIncompatibleModels([]));
    void core
      .listNonChatModels()
      .then(setOtherWeights)
      .catch(() => setOtherWeights([]));
    void core
      .modelSizes()
      .then(setTailles)
      .catch((e) => {
        console.warn("[modèles] tailles illisibles :", e);
        setTailles({});
      });
    void core
      .listDebridedModels()
      .then(setDebridedModels)
      .catch(() => setDebridedModels([]));
  }, []);

  // Qui revendique quoi. Le socle ignore à quoi sert un poids ; seule
  // l'extension qui le gère sait le reconnaître, par son catalogue.
  useEffect(() => {
    let cancelled = false;
    if (otherWeights.length === 0) {
      setWeightOwners({});
      return;
    }
    void loadExtensionMarketplaces(extensions, core.refreshExtensionAsset).then((catalogue) => {
      if (cancelled) return;
      const names = new Map(
        extensions.map((ext) => [ext.id, ext.display_name || ext.name] as const),
      );
      const owners: Record<string, string> = {};
      for (const weight of otherWeights) {
        const owner = claimantOf(weight.name, catalogue.claims);
        if (owner) owners[weight.name] = names.get(owner) ?? owner;
      }
      setWeightOwners(owners);
    });
    return () => {
      cancelled = true;
    };
  }, [extensions, otherWeights]);

  // Une seule liste. Séparer « modèles de conversation » et « poids
  // d'extension » en deux blocs demandait de chercher deux fois pour une
  // question qui n'en fait qu'une : qu'est-ce qui est installé, et où.
  const entries = useMemo<StoredEntry[]>(() => {
    const chat: StoredEntry[] = dedupedModels.map((rawTag) => {
      const cleanName = rawTag.replace(/^http.*[/\\]/, "").replace(/:latest$/, "");
      const nom = decrireModele(rawTag);
      return {
        tag: rawTag,
        kind: "chat",
        title: nom.titre,
        brand: "TEXTE · llama-server",
        group: "Conversation",
        tags: [...etiquettesDuNom(nom), tailles[rawTag] ? humanSize(tailles[rawTag]) : null].filter(
          (tag): tag is string => !!tag,
        ),
        path: `${modelsDir}\\${cleanName}`,
      };
    });

    const owned: StoredEntry[] = otherWeights.map((weight) => {
      const nom = decrireModele(weight.name);
      const owner = weightOwners[weight.name];
      return {
        tag: weight.name,
        kind: "extension",
        title: nom.titre,
        brand: owner ? owner.toUpperCase() : "AUCUNE EXTENSION",
        group: owner ?? "Non revendiqué",
        tags: [...etiquettesDuNom(nom), humanSize(weight.size_bytes)],
        path: `${modelsDir}\\${weight.name}`,
        hint: owner
          ? undefined
          : "Aucune extension installée ne revendique ces poids. Ils occupent de la place sans servir.",
      };
    });

    const broken: StoredEntry[] = incompatibleModels.map((name) => ({
      tag: name,
      kind: "incompatible",
      title: decrireModele(name).titre,
      brand: "TRANSFORMERS",
      group: "Non exécutable",
      tags: ["SAFETENSORS"],
      path: `${modelsDir}\\${name}`,
      hint: "Le moteur de conversation local ne charge pas ce dépôt. Gardé ici pour pouvoir libérer l'espace disque.",
    }));

    return [...chat, ...owned, ...broken].sort((a, b) =>
      a.title.localeCompare(b.title, "fr", { sensitivity: "base" }),
    );
  }, [dedupedModels, otherWeights, weightOwners, incompatibleModels, modelsDir, tailles]);

  // Ce que la ligne de chiffres annonce. La taille ne couvre que les poids
  // dont on connaît le compte d'octets : mieux vaut ne rien dire qu'annoncer
  // un total qui oublie la moitié du dossier.
  const diskTotal = useMemo(() => {
    const autres = otherWeights.reduce((sum, w) => sum + (w.size_bytes || 0), 0);
    // Les modèles de conversation aussi : ce sont eux qui pèsent le plus, et
    // le total les oubliait (deux modèles de 27B annoncés à 500 Mo).
    const chat = dedupedModels.reduce((sum, tag) => sum + (tailles[tag] ?? 0), 0);
    return humanSize(autres + chat);
  }, [otherWeights, dedupedModels, tailles]);

  const unclaimed = useMemo(
    () => entries.filter((e) => e.group === "Non revendiqué").length,
    [entries],
  );

  // Une puce par famille réellement présente, dans l'ordre où elles arrivent.
  const groups = useMemo(() => {
    const seen: string[] = [];
    for (const entry of entries) {
      if (!seen.includes(entry.group)) seen.push(entry.group);
    }
    return seen;
  }, [entries]);

  // Une famille qui disparaît — dernière extension retirée — ne doit pas
  // laisser un filtre actif sur du vide.
  useEffect(() => {
    if (groupFilter && !groups.includes(groupFilter)) setGroupFilter(null);
  }, [groups, groupFilter]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return entries.filter((entry) => {
      if (groupFilter && entry.group !== groupFilter) return false;
      if (riskFilter !== "all" && classifyModel(entry.tag).risk !== riskFilter) return false;
      if (!q) return true;
      return (
        entry.title.toLowerCase().includes(q) ||
        entry.tag.toLowerCase().includes(q) ||
        entry.brand.toLowerCase().includes(q)
      );
    });
  }, [entries, query, riskFilter, groupFilter]);

  async function handleUseForChat(model: string) {
    setActivatingModel(model);
    try {
      const providers = await core.listProviders();
      const active = providers.find((provider) => provider.is_active) ?? providers[0];
      if (active) await core.configureProvider(active.endpoint, model);
      onSelectModelForChat(model);
    } catch (error) {
      console.error("Failed to set active model:", error);
      onSelectModelForChat(model);
    } finally {
      setActivatingModel(null);
    }
  }

  async function handleOpenFolder(filePath?: string) {
    try {
      await core.openModelsFolder(filePath);
    } catch (error) {
      console.error("Failed to open folder:", error);
    }
  }

  async function handleToggleDebridage(entry: StoredEntry, active: boolean) {
    try {
      const updated = await core.toggleModelDebridage(entry.tag, active);
      setDebridedModels(updated);
      if (active) {
        setNoticeMessage(
          `Débridage activé pour « ${entry.title} ». Le modèle répondra sans restriction ni censure.`,
        );
      } else {
        setNoticeMessage(
          `Débridage retiré pour « ${entry.title} » (Mode éthique standard rétabli).`,
        );
      }
      setTimeout(() => setNoticeMessage(null), 4500);
    } catch (error) {
      console.error("Erreur lors de la modification du débridage:", error);
    } finally {
      setContextMenu(null);
    }
  }

  async function handleDelete(entry: StoredEntry) {
    if (!onDeleteModel) return;
    const detail =
      entry.kind === "incompatible"
        ? "Ce dépôt Transformers complet sera supprimé définitivement, y compris tous ses shards Safetensors."
        : entry.kind === "extension"
          ? "Ces poids seront supprimés définitivement. L'extension qui les utilise ne les retrouvera pas."
          : "Tous les shards de cette variante seront supprimés définitivement.";
    if (!(await confirmer(`Supprimer « ${entry.tag} » ?\n\n${detail}`))) return;
    await onDeleteModel(entry.tag);
    if (entry.kind === "incompatible") {
      setIncompatibleModels((current) => current.filter((item) => item !== entry.tag));
    }
    if (entry.kind === "extension") {
      setOtherWeights((current) => current.filter((item) => item.name !== entry.tag));
    }
  }

  const openedProvider = cloudProviders.find((p) => p.id === openFolder) ?? null;
  if (openedProvider) {
    return (
      <div className="locaryn-view-container">
        <CloudProviderScreen
          provider={openedProvider}
          extensions={extensions}
          onBack={() => setOpenFolder(null)}
          onChanged={reloadCloudProviders}
        />
      </div>
    );
  }

  return (
    <div className="locaryn-view-container">
      <div className="locaryn-view-header">
        <h2>Mes modèles installés</h2>
        <p className="locaryn-view-desc">
          Tout ce qui est stocké localement : les modèles de conversation et ceux qu'une extension
          utilise.
        </p>
        {/* Les chiffres en mono, sous la description : c'est du technique, et
            ça se lit d'un coup d'œil sans encombrer le titre. */}
        <div className="locaryn-view-stats">
          <span>
            {entries.length} modèle{entries.length > 1 ? "s" : ""}
          </span>
          {diskTotal && <span>{diskTotal} sur disque</span>}
          {unclaimed > 0 && (
            <span>
              {unclaimed} non revendiqué{unclaimed > 1 ? "s" : ""}
            </span>
          )}
        </div>
        <div className="locaryn-view-actions">
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => handleOpenFolder(modelsDir)}
          >
            <Icon name="project" size={16} /> Ouvrir le dossier
          </button>
          {onOpenMarketplace && (
            <button type="button" className="locaryn-btn-primary" onClick={onOpenMarketplace}>
              <Icon name="marketplace" size={16} /> Marketplace
            </button>
          )}
        </div>
      </div>

      {noticeMessage && (
        <div
          className="locaryn-card"
          style={{
            marginBottom: 16,
            padding: "10px 16px",
            background: "var(--accent-soft, rgba(99, 102, 241, 0.15))",
            border: "1px solid var(--accent, #6366f1)",
            display: "flex",
            alignItems: "center",
            gap: 10,
            fontSize: 13,
            fontWeight: 600,
          }}
        >
          <Icon name="sparkle" size={16} />
          <span style={{ flex: 1 }}>{noticeMessage}</span>
          <button
            type="button"
            className="locaryn-btn-ghost"
            style={{ padding: "2px 6px", fontSize: 11 }}
            onClick={() => setNoticeMessage(null)}
          >
            Fermer
          </button>
        </div>
      )}

      {groups.length > 1 && (
        <div
          style={{
            display: "flex",
            gap: 6,
            flexWrap: "wrap",
            alignItems: "center",
            marginBottom: 12,
          }}
        >
          <button
            type="button"
            className={`locaryn-chip${groupFilter === null ? " locaryn-chip-on" : ""}`}
            onClick={() => setGroupFilter(null)}
          >
            Tous ({entries.length})
          </button>
          {groups.map((group) => (
            <button
              key={group}
              type="button"
              className={`locaryn-chip${groupFilter === group ? " locaryn-chip-on" : ""}`}
              onClick={() => setGroupFilter((current) => (current === group ? null : group))}
            >
              {group} ({entries.filter((entry) => entry.group === group).length})
            </button>
          ))}
        </div>
      )}

      <div style={{ display: "flex", gap: 10, alignItems: "center", marginBottom: 16 }}>
        <input
          className="locaryn-input"
          style={{ flex: 1, fontSize: 13 }}
          placeholder="Filtrer mes modèles installés…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        {(["safe", "uncensored", "nsfw"] as const).map((risk) => (
          <button
            key={risk}
            type="button"
            className={`locaryn-chip${riskFilter === risk ? " locaryn-chip-on" : ""}`}
            onClick={() => setRiskFilter((current) => (current === risk ? "all" : risk))}
          >
            {risk === "safe" ? (
              <Icon name="shield" size={14} />
            ) : risk === "uncensored" ? (
              <Icon name="lock" size={14} />
            ) : (
              <Icon name="warning" size={14} />
            )}{" "}
            {risk === "safe" ? "Safe" : risk === "uncensored" ? "Sans limite" : "NSFW"}
          </button>
        ))}
      </div>

      {filtered.length === 0 && cloudProviders.length === 0 && (
        <div className="locaryn-card" style={{ textAlign: "center", padding: 48 }}>
          <Icon name="models" size={36} />
          <div style={{ fontSize: 15, fontWeight: 700, marginTop: 12 }}>
            {entries.length === 0
              ? "Aucun modèle installé localement"
              : "Aucun modèle ne correspond à votre recherche"}
          </div>
          {onOpenMarketplace && (
            <button
              type="button"
              className="locaryn-btn-primary"
              style={{ marginTop: 16 }}
              onClick={onOpenMarketplace}
            >
              <Icon name="marketplace" size={15} /> Télécharger un modèle
            </button>
          )}
        </div>
      )}

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))",
          gap: 12,
        }}
      >
        {/* Les dossiers d'abord : ce qui n'a rien à télécharger se trouve avant
            ce qui occupe le disque. */}
        {cloudProviders
          .filter((p) => !p.inline_models)
          .map((provider) => (
            <CloudProviderTile
              key={`cloud:${provider.id}`}
              provider={provider}
              onOpen={() => setOpenFolder(provider.id)}
            />
          ))}
        {telecharges.map((m) => (
          <div
            key={`direct:${m.provider.id}:${m.model.id}`}
            className="locaryn-box-card"
            style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10 }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
              <div style={{ minWidth: 0 }}>
                <BadgeEnLigne label={m.provider.label} />
                <h3
                  className="locaryn-box-name"
                  style={{ fontSize: 14, margin: "4px 0 0" }}
                  title={m.model.id}
                >
                  {m.model.name}
                </h3>
              </div>
              <span className="locaryn-tag locaryn-tag-installed" style={{ fontSize: 10 }}>
                Stocké
              </span>
            </div>
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginTop: "auto" }}>
              <button
                type="button"
                className="locaryn-btn-primary"
                style={{ flex: 1, fontSize: 12 }}
                disabled={occupe === m.model.id}
                onClick={() => void utiliserEnLigne(m)}
              >
                {m.provider.active_model === m.model.id
                  ? "Actif dans le chat"
                  : "Utiliser dans le chat"}
              </button>
              <button
                type="button"
                className="locaryn-btn-ghost"
                style={{ color: "var(--danger)", fontSize: 12 }}
                disabled={occupe === m.model.id}
                onClick={() => void supprimerEnLigne(m)}
              >
                <Icon name="trash" size={15} /> Supprimer
              </button>
            </div>
          </div>
        ))}
        {filtered.map((entry) => {
          const classification = classifyModel(entry.tag);
          const isDebrided = checkIsModelDebrided(entry, debridedModels);
          return (
            <div
              key={`${entry.kind}:${entry.tag}`}
              className="locaryn-box-card"
              style={{
                padding: 16,
                display: "flex",
                flexDirection: "column",
                gap: 10,
                cursor: "context-menu",
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                e.stopPropagation();
                ouvrirMenu({ x: e.clientX, y: e.clientY, entry });
              }}
            >
              <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
                <div style={{ minWidth: 0 }}>
                  <span className="locaryn-box-brand" style={{ fontSize: 10 }}>
                    {entry.brand}
                  </span>
                  {entry.kind === "chat" && (
                    <SpeedBadge metric={findMetric(metrics, entry.tag, "chat")} />
                  )}
                  {badgesEnLigne(enLigne, [entry.tag, entry.title]).map((label) => (
                    <span key={label} style={{ marginLeft: 6 }}>
                      <BadgeEnLigne label={label} />
                    </span>
                  ))}
                  {isDebrided && (
                    <span
                      className="locaryn-tag"
                      style={{
                        backgroundColor: "rgba(234, 179, 8, 0.18)",
                        color: "#facc15",
                        border: "1px solid rgba(234, 179, 8, 0.45)",
                        fontWeight: 700,
                        marginLeft: 6,
                        display: "inline-flex",
                        alignItems: "center",
                        gap: 3,
                      }}
                      title="Ce modèle est actuellement débridé (sans filtre ni censure)"
                    >
                      <Icon name="sparkle" size={12} /> Débridé
                    </span>
                  )}
                  {classification.risk !== "safe" && (
                    <span
                      className="locaryn-tag"
                      style={{ color: "var(--danger)", marginLeft: 6 }}
                      title={nsfwReason(entry.tag) ?? classification.label}
                    >
                      <Icon name={classification.icon} size={13} /> {classification.label}
                    </span>
                  )}
                  <h3
                    className="locaryn-box-name"
                    style={{ fontSize: 14, margin: "4px 0 0" }}
                    title={entry.tag}
                  >
                    {entry.title}
                  </h3>
                  <div style={{ display: "flex", gap: 5, flexWrap: "wrap", marginTop: 5 }}>
                    {entry.tags.map((tag) => (
                      <span key={tag} className="locaryn-tag locaryn-tag-soft">
                        {tag}
                      </span>
                    ))}
                  </div>
                </div>
                <span className="locaryn-tag locaryn-tag-installed" style={{ fontSize: 10 }}>
                  Stocké
                </span>
              </div>

              {entry.hint && (
                <p className="locaryn-field-hint" style={{ margin: 0 }}>
                  {entry.hint}
                </p>
              )}

              <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginTop: "auto" }}>
                {entry.kind === "chat" && (
                  <button
                    type="button"
                    className="locaryn-btn-primary"
                    style={{ flex: 1, fontSize: 12 }}
                    disabled={activatingModel === entry.tag}
                    onClick={() => void handleUseForChat(entry.tag)}
                  >
                    {activatingModel === entry.tag ? "Sélection…" : "Utiliser dans le chat"}
                  </button>
                )}
                {entry.kind === "incompatible" && onOpenMarketplace && (
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{ fontSize: 12 }}
                    onClick={onOpenMarketplace}
                  >
                    <Icon name="marketplace" size={15} /> Chercher un GGUF
                  </button>
                )}
                {onDeleteModel && (
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{ color: "var(--danger)", fontSize: 12 }}
                    onClick={() => void handleDelete(entry)}
                  >
                    <Icon name="trash" size={15} /> Supprimer
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {contextMenu &&
        (() => {
          const isCtxDebrided = checkIsModelDebrided(contextMenu.entry, debridedModels);
          return createPortal(
            <div
              className="locaryn-ctx"
              style={{
                position: "fixed",
                top: Math.max(10, Math.min(contextMenu.y, window.innerHeight - 200)),
                left: Math.max(10, Math.min(contextMenu.x, window.innerWidth - 260)),
                zIndex: 99999,
              }}
              ref={focusPremiereEntree}
              onClick={(e) => e.stopPropagation()}
              onKeyDown={navigationMenu}
              role="menu"
              tabIndex={-1}
            >
              <div className="locaryn-ctx-label" style={{ paddingBottom: "4px", fontWeight: 700 }}>
                {contextMenu.entry.title}
              </div>
              {contextMenu.entry.kind === "chat" && (
                <button
                  type="button"
                  role="menuitem"
                  className="locaryn-ctx-item"
                  onClick={() => {
                    const tag = contextMenu.entry.tag;
                    setContextMenu(null);
                    void handleUseForChat(tag);
                  }}
                >
                  <Icon name="chat" size={14} /> Utiliser dans le chat
                </button>
              )}
              {isCtxDebrided ? (
                <button
                  type="button"
                  role="menuitem"
                  className="locaryn-ctx-item"
                  style={{ fontSize: 13 }}
                  onClick={() => void handleToggleDebridage(contextMenu.entry, false)}
                >
                  <Icon name="shield" size={14} /> Rétablir par défaut (Éthique)
                </button>
              ) : (
                <button
                  type="button"
                  role="menuitem"
                  className="locaryn-ctx-item"
                  style={{ color: "var(--accent, #6366f1)", fontSize: 13, fontWeight: 600 }}
                  onClick={() => void handleToggleDebridage(contextMenu.entry, true)}
                >
                  <Icon name="sparkle" size={14} /> Ajouter un débridage
                </button>
              )}
              <div className="locaryn-ctx-sep" />
              <button
                type="button"
                role="menuitem"
                className="locaryn-ctx-item"
                onClick={() => {
                  const path = contextMenu.entry.path;
                  setContextMenu(null);
                  void handleOpenFolder(path);
                }}
              >
                <Icon name="project" size={14} /> Ouvrir l'emplacement
              </button>
              {onDeleteModel && (
                <button
                  type="button"
                  role="menuitem"
                  className="locaryn-ctx-item"
                  style={{ color: "var(--danger)" }}
                  onClick={() => {
                    const entryToDelete = contextMenu.entry;
                    setContextMenu(null);
                    void handleDelete(entryToDelete);
                  }}
                >
                  <Icon name="trash" size={14} /> Supprimer
                </button>
              )}
            </div>,
            document.body,
          );
        })()}
    </div>
  );
}
