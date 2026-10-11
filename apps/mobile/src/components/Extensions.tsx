import { Icon, capabilityLabel } from "@locaryn/ui-core";
import { useCallback, useEffect, useMemo, useState } from "react";
import { type Capability, type CatalogueEntry, type PhoneExtension, api } from "../lib/core";
import { useCoucheRetour } from "../lib/navigation";
import { Screen } from "./Screen";

type Props = {
  onBack: () => void;
  onChanged: () => void;
};

type Tab = "installed" | "catalog";
type InstalledFilter = "all" | "morphs" | "skills" | "cores";
type EcosystemFilter = "all" | "locaryn" | "gemini_cli" | "opencode";

/** Les permissions, dites comme une personne les comprend. */
const LIBELLES_PERMISSION: Record<string, string> = {
  shell: "Lancer des programmes",
  files_read: "Lire des fichiers",
  files_write: "Écrire des fichiers",
  network: "Accéder au réseau",
  extensions: "Gérer les extensions",
  mcp: "Démarrer ses connecteurs (MCP)",
  preview: "Ouvrir des aperçus",
  lsp: "Analyse de code (LSP)",
  env: "Lire les variables d'environnement",
};

function estExtension(v: unknown): v is PhoneExtension {
  return typeof v === "object" && v !== null && "name" in v && "enabled" in v;
}

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
      const resultat = await run();
      await reload();
      onChanged();
      // Une extension tout juste installée est éteinte : on propose aussitôt
      // de l'activer, permissions à l'appui — comme le bureau.
      if (estExtension(resultat) && !resultat.enabled) setPermissionTarget(resultat);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  /** « Autoriser et activer » : permissions accordées, extension active. */
  async function activer(ext: PhoneExtension) {
    setPermissionTarget(null);
    await handleAction(ext.name, () => api.activateExtension(ext.name));
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
      if (installedFilter === "skills" && ext.kind !== "plugin" && ext.ecosystem === "locaryn")
        return false;
      if (installedFilter === "morphs" && (ext.kind === "core" || ext.kind === "plugin"))
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

  // Le catalogue : relu au serveur à l'ouverture de l'onglet et à chaque
  // changement de recherche ou d'écosystème (le filtrage se fait là-bas).
  const [catalog, setCatalog] = useState<CatalogueEntry[] | null>(null);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  useEffect(() => {
    if (tab !== "catalog") return;
    let cancelled = false;
    const t = setTimeout(() => {
      api
        .extensionCatalog({ query: searchQuery, ecosystem: ecoFilter })
        .then((c) => {
          if (cancelled) return;
          setCatalog(c.entries);
          setCatalogError(null);
        })
        .catch((e) => {
          if (!cancelled) setCatalogError(String(e));
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [tab, searchQuery, ecoFilter]);
  const filteredCatalog = catalog ?? [];

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
            <Icon name="close" size={18} />
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
              className={`lo-chip ${installedFilter === "morphs" ? "lo-chip-active" : ""}`}
              onClick={() => setInstalledFilter("morphs")}
            >
              Extensions
            </button>
            <button
              type="button"
              className={`lo-chip ${installedFilter === "skills" ? "lo-chip-active" : ""}`}
              onClick={() => setInstalledFilter("skills")}
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
                        ext.enabled
                          ? handleAction(ext.name, () => api.setExtensionEnabled(ext.name, false))
                          : setPermissionTarget(ext)
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

          {catalogError && <p className="lo-error">{catalogError}</p>}
          {catalog === null && !catalogError && (
            <p className="lo-sub">Lecture du catalogue du serveur…</p>
          )}
          {catalog !== null && filteredCatalog.length === 0 && (
            <p className="lo-sub">Aucune extension ne correspond.</p>
          )}
          <ul className="lo-cards">
            {filteredCatalog.map((item) => {
              const isInstalled = item.installed || installedByName.has(item.name);
              const working = busy === item.install_source;

              return (
                <li
                  key={item.id}
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
                        <span className="lo-card-title">{item.display_name}</span>
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
                      {item.description && <span className="lo-hint">{item.description}</span>}
                      <span
                        style={{
                          fontSize: 11,
                          color: "var(--text-faint)",
                          marginTop: 2,
                          display: "block",
                        }}
                      >
                        <code>{item.install_source}</code>
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
                            handleAction(item.install_source, () =>
                              api.installExtension(item.install_source),
                            )
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
                <Icon name="close" size={18} />
              </button>
            </div>
            <form onSubmit={handleManualInstall}>
              <div className="lo-modal-body">
                <p className="lo-hint">
                  Indiquez un dépôt GitHub (ex: <code>proprietaire/depot</code>) ou une URL de
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
                <Icon name="close" size={18} />
              </button>
            </div>
            <div className="lo-modal-body">
              <p className="lo-hint">
                {permissionTarget.enabled
                  ? "Ce que cette extension peut faire sur le serveur :"
                  : "Pour fonctionner, cette extension demande :"}
              </p>
              {(permissionTarget.permissions ?? []).length === 0 ? (
                <p className="lo-hint">Aucune permission particulière.</p>
              ) : (
                <ul className="lo-permissions">
                  {(permissionTarget.permissions ?? []).map((p) => (
                    <li key={p.permission}>
                      <strong>{LIBELLES_PERMISSION[p.permission] ?? p.permission}</strong>
                      {p.reason && <span>{p.reason}</span>}
                      {permissionTarget.enabled && (
                        <em>{p.granted ? "accordée" : "non accordée"}</em>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </div>
            <div className="lo-modal-footer">
              {permissionTarget.enabled ? (
                <button type="button" className="lo-btn" onClick={() => setPermissionTarget(null)}>
                  Fermer
                </button>
              ) : (
                <>
                  <button
                    type="button"
                    className="lo-btn"
                    onClick={() => void activer(permissionTarget)}
                  >
                    Autoriser et activer
                  </button>
                  <button
                    type="button"
                    className="lo-btn-ghost"
                    onClick={() => setPermissionTarget(null)}
                  >
                    Plus tard
                  </button>
                </>
              )}
            </div>
          </div>
        </div>
      )}
    </Screen>
  );
}
