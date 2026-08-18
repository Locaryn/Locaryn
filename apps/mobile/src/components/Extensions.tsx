import { capabilityLabel } from "@locaryn/ui-core";
import { useCallback, useEffect, useMemo, useState } from "react";
import { CATALOGUE, type Capability, type PhoneExtension, api } from "../lib/core";
import { useCoucheRetour } from "../lib/navigation";
import { Screen } from "./Screen";

type Props = {
  onBack: () => void;
  onChanged: () => void;
};

type Tab = "installed" | "catalog";
type InstalledFilter = "all" | "extensions" | "plugins" | "cores";
type EcosystemFilter = "all" | "locaryn" | "gemini_cli" | "opencode";

interface CatalogItem {
  repo: string;
  name: string;
  label: string;
  note: string;
  ecosystem: "locaryn" | "gemini_cli" | "opencode";
  capabilities: string[];
}

const FULL_CATALOGUE: CatalogItem[] = [
  {
    repo: "Locaryn/plugin-image-gen",
    name: "plugin-image-gen",
    label: "Génération d'images",
    note: "Studio créatif et production visuelle par diffusion",
    ecosystem: "locaryn",
    capabilities: ["image-gen"],
  },
  {
    repo: "Locaryn/plugin-voice-tts",
    name: "plugin-voice-tts",
    label: "Voix de synthèse (TTS)",
    note: "Synthèse vocale réaliste et notes audio naturelles",
    ecosystem: "locaryn",
    capabilities: ["voice-tts"],
  },
  {
    repo: "Locaryn/plugin-image-editor",
    name: "plugin-image-editor",
    label: "Retouche d'image (Inpainting)",
    note: "Modification et retouche ciblée de zones d'images",
    ecosystem: "locaryn",
    capabilities: ["image-editor"],
  },
  {
    repo: "Locaryn/plugin-vision-ocr",
    name: "plugin-vision-ocr",
    label: "Vision & OCR",
    note: "Analyse d'images, extraction de texte et description visuelle",
    ecosystem: "locaryn",
    capabilities: ["vision-ocr"],
  },
  {
    repo: "Locaryn/plugin-translation",
    name: "plugin-translation",
    label: "Traduction multilingue",
    note: "Traduction précise en plus de 50 langues",
    ecosystem: "locaryn",
    capabilities: ["translation"],
  },
  {
    repo: "Locaryn/plugin-text-analysis",
    name: "plugin-text-analysis",
    label: "Analyse de texte & Résumé",
    note: "Extraction d'entités, synthèses de documents",
    ecosystem: "locaryn",
    capabilities: ["text-analysis"],
  },
  {
    repo: "Locaryn/plugin-rag-qa",
    name: "plugin-rag-qa",
    label: "Questions sur documents (RAG)",
    note: "Recherche sémantique et réponses sourcées sur vos fichiers",
    ecosystem: "locaryn",
    capabilities: ["rag-qa"],
  },
  {
    repo: "Locaryn/plugin-music-gen",
    name: "plugin-music-gen",
    label: "Génération de musique",
    note: "Composition musicale instrumentale et pistes audio",
    ecosystem: "locaryn",
    capabilities: ["music-gen"],
  },
  {
    repo: "Locaryn/plugin-video-gen",
    name: "plugin-video-gen",
    label: "Génération vidéo",
    note: "Création de courtes séquences vidéo et animations",
    ecosystem: "locaryn",
    capabilities: ["video-gen"],
  },
  {
    repo: "Locaryn/plugin-3d-gen",
    name: "plugin-3d-gen",
    label: "Objets 3D (Mesh & Splats)",
    note: "Génération d'objets 3D et rendus volumétriques",
    ecosystem: "locaryn",
    capabilities: ["3d-gen"],
  },
  {
    repo: "Locaryn/plugin-model-training",
    name: "plugin-model-training",
    label: "Entraînement LoRA & Oblitération",
    note: "Affinement de modèles et désapprentissage ciblé",
    ecosystem: "locaryn",
    capabilities: ["model-training"],
  },
  {
    repo: "Locaryn/plugin-ssh",
    name: "plugin-ssh",
    label: "Machine distante (SSH)",
    note: "Exécution de code et commandes sur serveurs distants",
    ecosystem: "locaryn",
    capabilities: ["ssh-terminal"],
  },
  {
    repo: "Locaryn/plugin-travel-tunnel",
    name: "plugin-travel-tunnel",
    label: "Mode Voyage sécurisé",
    note: "Accès au serveur depuis l'extérieur sans redirection de port",
    ecosystem: "locaryn",
    capabilities: ["travel-tunnel"],
  },
  {
    repo: "google/gemini-cli-tools",
    name: "gemini-cli-tools",
    label: "Gemini CLI Tools Bundle",
    note: "Commandes déclaratives et connecteurs compatibles Gemini CLI",
    ecosystem: "gemini_cli",
    capabilities: ["cli-tools"],
  },
  {
    repo: "opencode-ai/dev-pack",
    name: "opencode-dev-pack",
    label: "OpenCode Developer Skills",
    note: "Skills et agents d'ingénierie logicielle OpenCode",
    ecosystem: "opencode",
    capabilities: ["dev-skills"],
  },
];

const ECOSYSTEM_LABELS: Record<string, string> = {
  locaryn: "Officiel Locaryn",
  gemini_cli: "Gemini CLI",
  opencode: "OpenCode",
};

export function Extensions({ onBack, onChanged }: Props) {
  const [tab, setTab] = useState<Tab>("installed");
  const [installed, setInstalled] = useState<PhoneExtension[] | null>(null);
  const [canonique, setCanonique] = useState<Capability[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [installedFilter, setInstalledFilter] = useState<InstalledFilter>("all");
  const [ecoFilter, setEcoFilter] = useState<EcosystemFilter>("all");

  // Modal d'installation manuelle
  const [showInstallModal, setShowInstallModal] = useState(false);
  // Le retour d'Android referme les fenêtres ouvertes avant de quitter.
  useCoucheRetour(showInstallModal, () => setShowInstallModal(false));
  const [manualSource, setManualSource] = useState("");

  // Modal de permissions
  const [permissionTarget, setPermissionTarget] = useState<PhoneExtension | null>(null);
  useCoucheRetour(permissionTarget !== null, () => setPermissionTarget(null));

  const reload = useCallback(async () => {
    try {
      const [exts, caps] = await Promise.all([
        api.listExtensions(),
        api.listCapabilities().catch(() => []),
      ]);
      setInstalled(exts);
      setCanonique(caps);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function handleAction(key: string, run: () => Promise<unknown>) {
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

  async function handleManualInstall(e: React.FormEvent) {
    e.preventDefault();
    const src = manualSource.trim();
    if (!src) return;
    setShowInstallModal(false);
    setManualSource("");
    await handleAction(src, () => api.installExtension(src));
  }

  const installedByName = useMemo(() => {
    return new Map((installed ?? []).map((e) => [e.name, e]));
  }, [installed]);

  // Filtrage des extensions installées
  const filteredInstalled = useMemo(() => {
    if (!installed) return [];
    return installed.filter((ext) => {
      if (installedFilter === "cores" && ext.kind !== "core") return false;
      if (installedFilter === "plugins" && ext.kind !== "plugin" && ext.ecosystem === "locaryn")
        return false;
      if (installedFilter === "extensions" && (ext.kind === "core" || ext.kind === "plugin"))
        return false;
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchName = ext.name.toLowerCase().includes(q);
        const matchDisplay = ext.display_name?.toLowerCase().includes(q);
        const matchDesc = ext.description?.toLowerCase().includes(q);
        if (!matchName && !matchDisplay && !matchDesc) return false;
      }
      return true;
    });
  }, [installed, installedFilter, searchQuery]);

  // Filtrage du catalogue
  const filteredCatalog = useMemo(() => {
    return FULL_CATALOGUE.filter((item) => {
      if (ecoFilter !== "all" && item.ecosystem !== ecoFilter) return false;
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchLabel = item.label.toLowerCase().includes(q);
        const matchNote = item.note.toLowerCase().includes(q);
        const matchRepo = item.repo.toLowerCase().includes(q);
        if (!matchLabel && !matchNote && !matchRepo) return false;
      }
      return true;
    });
  }, [ecoFilter, searchQuery]);

  return (
    <Screen
      title="Extensions"
      onBack={onBack}
      action={
        <button type="button" className="lo-bar-action" onClick={() => setShowInstallModal(true)}>
          + Installer
        </button>
      }
    >
      <div className="lo-tabs">
        <button
          type="button"
          className={`lo-tab ${tab === "installed" ? "lo-tab-active" : ""}`}
          onClick={() => setTab("installed")}
        >
          Installées ({installed?.length ?? 0})
        </button>
        <button
          type="button"
          className={`lo-tab ${tab === "catalog" ? "lo-tab-active" : ""}`}
          onClick={() => setTab("catalog")}
        >
          Découvrir / Catalogue
        </button>
      </div>

      <div className="lo-search-box">
        <input
          type="text"
          className="lo-search-input"
          placeholder="Rechercher une extension, un plugin ou une capacité…"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
        />
        {searchQuery && (
          <button
            type="button"
            className="lo-btn-ghost"
            style={{
              position: "absolute",
              right: 8,
              width: "auto",
              minHeight: "auto",
              padding: 4,
              border: "none",
            }}
            onClick={() => setSearchQuery("")}
          >
            ✕
          </button>
        )}
      </div>

      {error && <p className="lo-error">{error}</p>}

      {/* ── Onglet : Installées ── */}
      {tab === "installed" && (
        <>
          <div className="lo-chips">
            <button
              type="button"
              className={`lo-chip ${installedFilter === "all" ? "lo-chip-active" : ""}`}
              onClick={() => setInstalledFilter("all")}
            >
              Toutes
            </button>
            <button
              type="button"
              className={`lo-chip ${installedFilter === "extensions" ? "lo-chip-active" : ""}`}
              onClick={() => setInstalledFilter("extensions")}
            >
              Extensions
            </button>
            <button
              type="button"
              className={`lo-chip ${installedFilter === "plugins" ? "lo-chip-active" : ""}`}
              onClick={() => setInstalledFilter("plugins")}
            >
              Plugins compatibles
            </button>
            <button
              type="button"
              className={`lo-chip ${installedFilter === "cores" ? "lo-chip-active" : ""}`}
              onClick={() => setInstalledFilter("cores")}
            >
              Noyaux
            </button>
          </div>

          {installed === null && !error && <p className="lo-sub">Chargement des extensions…</p>}

          {installed !== null && filteredInstalled.length === 0 && (
            <p className="lo-sub" style={{ textAlign: "center", marginTop: "var(--space-4)" }}>
              Aucune extension installée dans cette catégorie.
            </p>
          )}

          <ul className="lo-cards">
            {filteredInstalled.map((ext) => {
              const working = busy === ext.name;
              return (
                <li
                  key={ext.name}
                  className="lo-card"
                  style={{ flexDirection: "column", alignItems: "stretch" }}
                >
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "flex-start",
                      gap: 8,
                    }}
                  >
                    <div className="lo-card-text">
                      <div
                        style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}
                      >
                        <span className="lo-card-title">{ext.display_name || ext.name}</span>
                        <span style={{ fontSize: 11, color: "var(--text-faint)" }}>
                          v{ext.version || "0.1"}
                        </span>
                        {ext.ecosystem && (
                          <span
                            style={{
                              fontSize: 10,
                              padding: "1px 6px",
                              borderRadius: 4,
                              background: "rgba(255, 255, 255, 0.08)",
                              color: "var(--text-dim)",
                            }}
                          >
                            {ECOSYSTEM_LABELS[ext.ecosystem] ?? ext.ecosystem}
                          </span>
                        )}
                      </div>
                      {ext.description && <span className="lo-hint">{ext.description}</span>}
                    </div>

                    <span
                      className={`lo-tag ${ext.enabled ? "lo-tag-on" : ""}`}
                      style={{
                        padding: "2px 8px",
                        fontSize: 11,
                        borderRadius: 10,
                        background: ext.enabled
                          ? "rgba(var(--accent-rgb), 0.2)"
                          : "rgba(255, 255, 255, 0.08)",
                        color: ext.enabled ? "var(--accent)" : "var(--text-faint)",
                        fontWeight: 600,
                        flex: "none",
                      }}
                    >
                      {ext.enabled ? "Active" : "Désactivée"}
                    </span>
                  </div>

                  {ext.capabilities.length > 0 && (
                    <div style={{ display: "flex", gap: 4, flexWrap: "wrap", marginTop: 4 }}>
                      {ext.capabilities.map((cap) => {
                        const label =
                          canonique.find((c) => c.id === cap)?.label ?? capabilityLabel(cap);
                        return (
                          <span
                            key={cap}
                            style={{
                              fontSize: 11,
                              padding: "2px 6px",
                              background: "rgba(var(--accent-rgb), 0.1)",
                              color: "var(--accent)",
                              borderRadius: 4,
                            }}
                          >
                            {label}
                          </span>
                        );
                      })}
                    </div>
                  )}

                  <div
                    style={{
                      display: "flex",
                      justifyContent: "flex-end",
                      gap: 6,
                      marginTop: 8,
                      paddingTop: 8,
                      borderTop: "1px solid rgba(255, 255, 255, 0.05)",
                    }}
                  >
                    <button
                      type="button"
                      className="lo-btn-small"
                      disabled={working}
                      onClick={() =>
                        handleAction(ext.name, () =>
                          api.setExtensionEnabled(ext.name, !ext.enabled),
                        )
                      }
                    >
                      {ext.enabled ? "Désactiver" : "Activer"}
                    </button>
                    <button
                      type="button"
                      className="lo-btn-small"
                      onClick={() => setPermissionTarget(ext)}
                    >
                      Permissions
                    </button>
                    <button
                      type="button"
                      className="lo-btn-small"
                      style={{ color: "var(--danger)" }}
                      disabled={working}
                      onClick={() => handleAction(ext.name, () => api.removeExtension(ext.name))}
                    >
                      {working ? "Suppression…" : "Supprimer"}
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        </>
      )}

      {/* ── Onglet : Découvrir / Catalogue ── */}
      {tab === "catalog" && (
        <>
          <div className="lo-chips">
            <button
              type="button"
              className={`lo-chip ${ecoFilter === "all" ? "lo-chip-active" : ""}`}
              onClick={() => setEcoFilter("all")}
            >
              Tous les écosystèmes
            </button>
            <button
              type="button"
              className={`lo-chip ${ecoFilter === "locaryn" ? "lo-chip-active" : ""}`}
              onClick={() => setEcoFilter("locaryn")}
            >
              Officiel Locaryn
            </button>
            <button
              type="button"
              className={`lo-chip ${ecoFilter === "gemini_cli" ? "lo-chip-active" : ""}`}
              onClick={() => setEcoFilter("gemini_cli")}
            >
              Gemini CLI
            </button>
            <button
              type="button"
              className={`lo-chip ${ecoFilter === "opencode" ? "lo-chip-active" : ""}`}
              onClick={() => setEcoFilter("opencode")}
            >
              OpenCode
            </button>
          </div>

          <p className="lo-hint" style={{ margin: "4px 0 8px 0" }}>
            Les extensions s'installent sur votre serveur et deviennent disponibles pour tous vos
            appareils connectés.
          </p>

          <ul className="lo-cards">
            {filteredCatalog.map((item) => {
              const installedExt = installedByName.get(item.name);
              const isInstalled = Boolean(installedExt);
              const working = busy === item.repo;

              return (
                <li
                  key={item.repo}
                  className="lo-card"
                  style={{ flexDirection: "column", alignItems: "stretch" }}
                >
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "flex-start",
                      gap: 8,
                    }}
                  >
                    <div className="lo-card-text">
                      <div
                        style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}
                      >
                        <span className="lo-card-title">{item.label}</span>
                        <span
                          style={{
                            fontSize: 10,
                            padding: "1px 6px",
                            borderRadius: 4,
                            background: "rgba(255, 255, 255, 0.08)",
                            color: "var(--text-dim)",
                          }}
                        >
                          {ECOSYSTEM_LABELS[item.ecosystem] ?? item.ecosystem}
                        </span>
                      </div>
                      <span className="lo-hint">{item.note}</span>
                      <span
                        style={{
                          fontSize: 11,
                          color: "var(--text-faint)",
                          marginTop: 2,
                          display: "block",
                        }}
                      >
                        <code>{item.repo}</code>
                      </span>
                    </div>

                    <div style={{ flex: "none" }}>
                      {isInstalled ? (
                        <span
                          style={{
                            fontSize: 11,
                            padding: "3px 8px",
                            borderRadius: 10,
                            background: "rgba(var(--accent-rgb), 0.15)",
                            color: "var(--accent)",
                            fontWeight: 600,
                          }}
                        >
                          Installée
                        </span>
                      ) : (
                        <button
                          type="button"
                          className="lo-btn-small lo-btn-small-on"
                          disabled={working}
                          onClick={() =>
                            handleAction(item.repo, () => api.installExtension(item.repo))
                          }
                        >
                          {working ? "Installation…" : "Installer"}
                        </button>
                      )}
                    </div>
                  </div>
                </li>
              );
            })}
          </ul>
        </>
      )}

      {/* ── Modal d'installation manuelle ── */}
      {showInstallModal && (
        <div className="lo-modal-backdrop" onClick={() => setShowInstallModal(false)}>
          <div className="lo-modal" onClick={(e) => e.stopPropagation()}>
            <div className="lo-modal-header">
              <span className="lo-modal-title">Installer une extension</span>
              <button
                type="button"
                className="lo-btn-ghost"
                style={{ width: "auto", minHeight: "auto", padding: "4px 8px", border: "none" }}
                onClick={() => setShowInstallModal(false)}
              >
                ✕
              </button>
            </div>
            <form onSubmit={handleManualInstall}>
              <div className="lo-modal-body">
                <p className="lo-hint">
                  Indiquez un dépôt GitHub (ex: <code>Locaryn/plugin-image-gen</code>) ou une URL de
                  paquet d'extension.
                </p>
                <div>
                  <label className="lo-label">Source ou Dépôt Git</label>
                  <input
                    type="text"
                    className="lo-input"
                    placeholder="ex: organisation/depot"
                    value={manualSource}
                    onChange={(e) => setManualSource(e.target.value)}
                    autoFocus
                    required
                  />
                </div>
              </div>
              <div className="lo-modal-footer">
                <button type="submit" className="lo-btn" disabled={!manualSource.trim()}>
                  Lancer l'installation
                </button>
                <button
                  type="button"
                  className="lo-btn-ghost"
                  onClick={() => setShowInstallModal(false)}
                >
                  Annuler
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* ── Modal de permissions ── */}
      {permissionTarget && (
        <div className="lo-modal-backdrop" onClick={() => setPermissionTarget(null)}>
          <div className="lo-modal" onClick={(e) => e.stopPropagation()}>
            <div className="lo-modal-header">
              <span className="lo-modal-title">
                Permissions : {permissionTarget.display_name || permissionTarget.name}
              </span>
              <button
                type="button"
                className="lo-btn-ghost"
                style={{ width: "auto", minHeight: "auto", padding: "4px 8px", border: "none" }}
                onClick={() => setPermissionTarget(null)}
              >
                ✕
              </button>
            </div>
            <div className="lo-modal-body">
              <p className="lo-hint">
                Cette extension s'exécute dans un environnement sécurisé sur le serveur avec les
                autorisations accordées :
              </p>
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <div
                  style={{
                    padding: "8px 12px",
                    background: "var(--surface)",
                    borderRadius: "var(--radius-sm)",
                  }}
                >
                  <div style={{ fontWeight: 600, fontSize: 13, color: "var(--text)" }}>
                    Capacités déclarées
                  </div>
                  <div style={{ fontSize: 12, color: "var(--text-dim)", marginTop: 2 }}>
                    {permissionTarget.capabilities.length > 0
                      ? permissionTarget.capabilities.join(", ")
                      : "Aucune capacité particulière requise"}
                  </div>
                </div>
                <div
                  style={{
                    padding: "8px 12px",
                    background: "var(--surface)",
                    borderRadius: "var(--radius-sm)",
                  }}
                >
                  <div style={{ fontWeight: 600, fontSize: 13, color: "var(--text)" }}>
                    Niveau de sécurité
                  </div>
                  <div style={{ fontSize: 12, color: "var(--accent)", marginTop: 2 }}>
                    Vérifié & Isolé (Sandbox Daemon)
                  </div>
                </div>
              </div>
            </div>
            <div className="lo-modal-footer">
              <button type="button" className="lo-btn" onClick={() => setPermissionTarget(null)}>
                Fermer
              </button>
            </div>
          </div>
        </div>
      )}
    </Screen>
  );
}
