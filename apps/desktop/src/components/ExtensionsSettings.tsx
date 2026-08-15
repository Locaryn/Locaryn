import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  type CatalogEntry,
  type CatalogSnapshot,
  type CatalogSource,
  ECOSYSTEM_LABELS,
  type ExtensionEcosystem,
  type ExtensionPermission,
  type ExtensionUpdateCheck,
  type InstalledExtension,
  PERMISSION_LABELS,
  core,
} from "../lib/core";
import { consumePendingInstall, subscribeDeepLink } from "../lib/deepLink";
import { ExtensionConfigPanel } from "./ExtensionConfigPanel";
import { ExtensionInstallDialog } from "./ExtensionInstallDialog";
import { ExtensionPermissionsModal } from "./ExtensionPermissionsModal";

/**
 * The extension store, wired to the real registry.
 *
 * The previous screen listed hard-coded cards whose install button toggled a
 * React state variable, so nothing was ever installed and nothing ever ran.
 * Every action here goes through the backend: installing fetches and adapts a
 * bundle, permissions are stored, and enabling registers the plugin's MCP
 * servers, rules and commands with the runtimes.
 */

const ECOSYSTEM_FILTERS: { id: ExtensionEcosystem | "all"; label: string }[] = [
  { id: "all", label: "Tous" },
  { id: "locaryn", label: "Officiel Locaryn (Certifié)" },
  { id: "claude_code", label: ECOSYSTEM_LABELS.claude_code },
  { id: "gemini_cli", label: ECOSYSTEM_LABELS.gemini_cli },
  { id: "opencode", label: ECOSYSTEM_LABELS.opencode },
  { id: "mcp", label: ECOSYSTEM_LABELS.mcp },
];

/** Combien d'extensions mises à jour en même temps pendant le lot. */
const BATCH_CONCURRENCY = 3;

/** Compare deux versions par segments numériques (miroir du `version_gt` Rust). */
function versionGt(a: string, b: string): boolean {
  const sa = a.split(".").map((s) => Number.parseInt(s, 10) || 0);
  const sb = b.split(".").map((s) => Number.parseInt(s, 10) || 0);
  const n = Math.max(sa.length, sb.length);
  for (let i = 0; i < n; i++) {
    const x = sa[i] ?? 0;
    const y = sb[i] ?? 0;
    if (x !== y) return x > y;
  }
  return false;
}

/** Une cible du lot : extension installée (source enregistrée) ou entrée du
 *  catalogue « Découvrir » (source explicite). */
type BatchTarget = {
  name: string;
  /** Vérifié « déjà à jour » : à sauter, sauf si le catalogue annonce plus. */
  skip: boolean;
  run: () => Promise<InstalledExtension>;
};

/** What each compatibility level means, said plainly rather than with a colour. */
const COMPAT: Record<string, { label: string; hint: string }> = {
  native: { label: "Compatible", hint: "S'installe et fonctionne tel quel." },
  adapted: {
    label: "Converti",
    hint: "Format étranger, entièrement déclaratif : converti sans perte à l'installation.",
  },
  partial: {
    label: "Partiel",
    hint: "Une partie du paquet ne peut pas fonctionner ici. Le détail est indiqué après l'installation.",
  },
  unsupported: {
    label: "Non exécutable",
    hint: "Listé pour la recherche : rien dans ce paquet ne peut tourner dans Locaryn.",
  },
};

function componentSummary(e: InstalledExtension): string {
  const c = e.components;
  const parts: string[] = [];
  if (c.commands) parts.push(`${c.commands} commande${c.commands > 1 ? "s" : ""}`);
  if (c.skills) parts.push(`${c.skills} skill${c.skills > 1 ? "s" : ""}`);
  if (c.agents) parts.push(`${c.agents} agent${c.agents > 1 ? "s" : ""}`);
  if (c.mcp_servers) parts.push(`${c.mcp_servers} serveur MCP`);
  if (c.rules) parts.push(`${c.rules} règle${c.rules > 1 ? "s" : ""}`);
  if (c.hooks) parts.push(`${c.hooks} hook${c.hooks > 1 ? "s" : ""}`);
  if (c.lsp_adapters) parts.push(`${c.lsp_adapters} LSP`);
  return parts.length ? parts.join(" · ") : "aucun composant";
}

export function ExtensionsSettings() {
  const [tab, setTab] = useState<"installed" | "browse">("installed");
  const [installed, setInstalled] = useState<InstalledExtension[]>([]);
  const [snapshot, setSnapshot] = useState<CatalogSnapshot | null>(null);
  const [sources, setSources] = useState<CatalogSource[]>([]);
  const [ecosystem, setEcosystem] = useState<ExtensionEcosystem | "all">("all");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const [configuring, setConfiguring] = useState<InstalledExtension | null>(null);
  // Fenêtre d'ajout (dépôt / dossier / ZIP, ou marketplace) et fenêtre de
  // permissions — des composants partagés avec le panneau Connecteurs.
  const [installDialog, setInstallDialog] = useState<{
    open: boolean;
    kind: "extension" | "marketplace";
    /** Source pré-remplie (deep link locaryn://install?src=…). */
    initialSource?: string;
  }>({ open: false, kind: "extension" });
  const [permissionExt, setPermissionExt] = useState<{
    ext: InstalledExtension;
    grants: Set<ExtensionPermission>;
    /** "install" = extension fraîchement installée ; "edit" = permissions déjà accordées. */
    ctx: "install" | "edit";
  } | null>(null);
  // Version disponible sur la source GitHub, par id d'extension. Rempli au
  // chargement et après chaque mise à jour ; vide hors-ligne ou pour les
  // sources non vérifiables (chemin local, ref épinglée).
  const [updates, setUpdates] = useState<Record<string, ExtensionUpdateCheck>>({});
  // TTL en mémoire : ne pas re-solliciter raw.githubusercontent.com à chaque
  // ouverture du panneau. `true` force le contrôle (ex. après une mise à jour).
  const updatesCheckedAtRef = useRef(0);
  // Mise à jour en lot. `busy` est par-id, donc un drapeau global verrouille
  // toutes les cartes pendant que le lot tourne.
  const [updatingAll, setUpdatingAll] = useState(false);
  const [batchProgress, setBatchProgress] = useState<{
    total: number;
    done: number;
    /** Extensions en cours (jusqu'à BATCH_CONCURRENCY à la fois). */
    current: string[];
  } | null>(null);
  const [batchReport, setBatchReport] = useState<{
    updated: string[];
    skipped: number;
    failed: { name: string; reason: string }[];
    /** Vrai quand l'utilisateur a annulé en cours de route. */
    cancelled: boolean;
  } | null>(null);
  // Drapeau d'annulation du lot : vérifié entre chaque extension, jamais
  // pendant une mise à jour — l'itération en cours va jusqu'au bout.
  const batchCancelRef = useRef(false);

  // Échap ferme la fenêtre de permissions (installation ou édition). La
  // fenêtre d'ajout gère la sienne.
  useEffect(() => {
    if (!permissionExt) return;
    const onKey = (ev: KeyboardEvent) => {
      if (ev.key === "Escape") setPermissionExt(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [permissionExt]);

  // Lien locaryn://install?src=… : l'intention est posée par App et consommée
  // ici — ouvre la fenêtre d'ajout avec la source pré-remplie. Vérifiée au
  // montage (le panneau peut arriver après le lien) puis à chaque événement.
  useEffect(() => {
    const openFromLink = () => {
      const intent = consumePendingInstall();
      if (intent) {
        setInstallDialog({
          open: true,
          kind: "extension",
          initialSource: intent.source,
        });
      }
    };
    openFromLink();
    return subscribeDeepLink(openFromLink);
  }, []);

  const loadInstalled = useCallback(async () => {
    try {
      setInstalled(await core.listExtensions());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  /** Compare les versions installées à la source GitHub. Silencieux hors-ligne. */
  const refreshUpdates = useCallback(async (force = false) => {
    const now = Date.now();
    if (!force && now - updatesCheckedAtRef.current < 5 * 60_000) return;
    try {
      const list = await core.checkExtensionUpdates();
      // Estampille seulement au succès : un échec réseau ne bloque pas la
      // prochaine ouverture du panneau pendant 5 minutes.
      updatesCheckedAtRef.current = now;
      setUpdates(Object.fromEntries(list.map((u) => [u.id, u])));
    } catch {
      setUpdates({});
    }
  }, []);

  const loadCatalog = useCallback(async () => {
    try {
      setSnapshot(
        await core.browseExtensionCatalog({
          query,
          ecosystem: ecosystem === "all" ? null : ecosystem,
          limit: 60,
        }),
      );
    } catch (e) {
      setError(String(e));
    }
  }, [query, ecosystem]);

  useEffect(() => {
    loadInstalled();
    refreshUpdates();
    core
      .listCatalogSources()
      .then(setSources)
      .catch(() => setSources([]));
  }, [loadInstalled, refreshUpdates]);

  useEffect(() => {
    if (tab !== "browse") return;
    const t = setTimeout(loadCatalog, 180);
    return () => clearTimeout(t);
  }, [tab, loadCatalog]);

  async function refreshCatalog() {
    setRefreshing(true);
    setError(null);
    try {
      const snap = await core.refreshExtensionCatalog();
      const failed = snap.sources.filter((s) => !s.ok);
      if (failed.length) {
        setError(
          `Sources injoignables : ${failed
            .map((s) => `${s.source.label} (${s.error ?? "erreur"})`)
            .join(", ")}`,
        );
      }
      if (snap.stale) {
        setNotice("Aucune source n'a répondu — affichage de la dernière copie locale.");
      }
      await loadCatalog();
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(false);
    }
  }

  /** Termine une installation : liste rafraîchie, badges, notice. */
  async function finishInstall(ext: InstalledExtension, enable: boolean) {
    await loadInstalled();
    await refreshUpdates(true);
    if (tab === "browse") await loadCatalog();
    setNotice(
      enable
        ? `${ext.name} installée et activée — ${componentSummary(ext)}.`
        : `${ext.name} installée, désactivée pour l'instant.`,
    );
  }

  /** Installation depuis une carte du catalogue (« Installer »). */
  async function installFromCatalog(entry: CatalogEntry) {
    setBusy(entry.id);
    setError(null);
    setNotice(null);
    try {
      const ext = await core.installExtension(entry.install_source);
      await loadInstalled();
      if (ext.permissions.length === 0) {
        await core.setExtensionEnabled(ext.id, true);
        await finishInstall(ext, true);
      } else {
        setPermissionExt({
          ext,
          grants: new Set(ext.permissions.map((p) => p.permission)),
          ctx: "install",
        });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  /** Fin de la fenêtre de permissions (installation du catalogue ou édition). */
  async function handlePermissionsDone(ext: InstalledExtension, enable: boolean) {
    const ctx = permissionExt?.ctx ?? "edit";
    setPermissionExt(null);
    if (ctx === "install") {
      await finishInstall(ext, enable);
    } else {
      await loadInstalled();
      setNotice(`Permissions de ${ext.name} enregistrées.`);
    }
  }

  async function toggleEnabled(e: InstalledExtension) {
    setBusy(e.id);
    setError(null);
    try {
      setInstalled(await core.setExtensionEnabled(e.id, !e.enabled));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  }

  /** Re-installe depuis la source enregistrée (github:…, chemin ou zip).
   *  L'upsert garde l'id, l'état actif et les permissions accordées. */
  async function update(e: InstalledExtension) {
    setBusy(e.id);
    setError(null);
    setNotice(null);
    try {
      const next = await core.updateExtension(e.id);
      await loadInstalled();
      await refreshUpdates(true);
      setNotice(`${next.name} mise à jour — v${next.version}.`);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  }

  /** Met à jour en séquence chaque extension qui a une source enregistrée.
   *  Une extension vérifiée comme déjà à jour est sautée ; les échecs sont
   *  collectés et rapportés, sans interrompre le lot. */
  /** Demande l'arrêt du lot après l'itération en cours. */
  function cancelBatch() {
    batchCancelRef.current = true;
  }

  async function updateAll() {
    const targets = batchTargets.filter((t) => !t.skip);
    if (targets.length === 0) return;
    batchCancelRef.current = false;
    setUpdatingAll(true);
    setError(null);
    setNotice(null);
    setBatchReport(null);
    const skipped = batchTargets.length - targets.length;

    const updated: string[] = [];
    const failed: { name: string; reason: string }[] = [];
    let cancelled = false;

    // Progression globale : `finished` compte les extensions terminées,
    // `inFlight` les noms encore en cours. Mis à jour entre chaque await.
    let nextIndex = 0;
    let finished = 0;
    const inFlight = new Set<string>();
    const refresh = () =>
      setBatchProgress({ total: targets.length, done: finished, current: [...inFlight] });

    // Un worker tire la prochaine cible. L'annulation est vérifiée avant
    // chaque nouvelle extension : celles déjà lancées vont jusqu'au bout,
    // puis tous les workers s'arrêtent proprement.
    const worker = async () => {
      for (;;) {
        if (batchCancelRef.current) {
          cancelled = true;
          return;
        }
        const t = targets[nextIndex++];
        if (!t) return;
        inFlight.add(t.name);
        refresh();
        try {
          await t.run();
          updated.push(t.name);
        } catch (err) {
          failed.push({ name: t.name, reason: String(err) });
        }
        inFlight.delete(t.name);
        finished++;
        refresh();
      }
    };

    await Promise.all(
      Array.from({ length: Math.min(BATCH_CONCURRENCY, targets.length) }, () => worker()),
    );

    setBatchProgress(null);
    setUpdatingAll(false);
    // Les reloads globaux des mises à jour parallèles peuvent se chevaucher :
    // un dernier reload unique garantit un état final cohérent.
    try {
      await core.reloadExtensions();
    } catch {
      // Le panneau se rafraîchit quand même par loadInstalled ci-dessous.
    }
    await loadInstalled();
    await refreshUpdates(true);
    setBatchReport({ updated, skipped, failed, cancelled });
  }

  async function remove(e: InstalledExtension) {
    if (!window.confirm(`Désinstaller « ${e.name} » et supprimer ses fichiers ?`)) return;
    setBusy(e.id);
    try {
      setInstalled(await core.removeExtension(e.id));
      await refreshUpdates(true);
      if (tab === "browse") await loadCatalog();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  }

  const grouped = useMemo(() => {
    const map = new Map<ExtensionEcosystem, InstalledExtension[]>();
    for (const e of installed) {
      const list = map.get(e.ecosystem) ?? [];
      list.push(e);
      map.set(e.ecosystem, list);
    }
    return [...map.entries()];
  }, [installed]);

  const entries = snapshot?.entries ?? [];
  // Entrées du catalogue déjà installées, par nom — pour repérer une version
  // plus récente que le contrôle GitHub ne voit pas (source locale, etc.).
  const catalogByName = useMemo(() => {
    const m = new Map<string, CatalogEntry>();
    for (const c of entries) if (c.installed) m.set(c.name, c);
    return m;
  }, [entries]);

  // Cibles du lot : les extensions installées avec une source enregistrée, plus
  // les entrées du catalogue déjà installées qui annoncent une version plus
  // récente (source enregistrée manquante). `skip` = vérifié déjà à jour.
  const batchTargets = useMemo<BatchTarget[]>(() => {
    const installedByName = new Map(installed.map((e) => [e.name, e]));
    const out: BatchTarget[] = [];
    for (const e of installed) {
      if (!e.source) continue;
      const u = updates[e.id];
      const verifiedCurrent = !!u && u.update_available === false && !u.error;
      const cat = catalogByName.get(e.name);
      const catNewer = cat?.version != null && versionGt(cat.version, e.version);
      out.push({
        name: e.name,
        skip: verifiedCurrent && !catNewer,
        run: () => core.updateExtension(e.id),
      });
    }
    const covered = new Set(out.map((t) => t.name));
    for (const c of entries) {
      if (!c.installed || !c.install_source || c.compat === "unsupported") continue;
      const ext = installedByName.get(c.name);
      // Déjà couverte par une source enregistrée, ou installation inconnue.
      if (!ext || ext.source || covered.has(c.name)) continue;
      // Sans version catalogue plus récente, on ne sait pas si c'est à jour.
      if (c.version == null || !versionGt(c.version, ext.version)) continue;
      out.push({
        name: c.name,
        skip: false,
        run: () => core.updateExtensionSource(ext.id, c.install_source),
      });
      // Une même extension listée dans plusieurs marketplaces ne doit pas être
      // poussée deux fois dans le lot.
      covered.add(c.name);
    }
    return out;
  }, [installed, entries, updates, catalogByName]);
  const pendingCount = batchTargets.filter((t) => !t.skip).length;

  return (
    <div className="locaryn-ext-settings">
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          gap: 12,
          marginBottom: 16,
          flexWrap: "wrap",
        }}
      >
        <div style={{ display: "flex", gap: 8 }}>
          <button
            type="button"
            className={`locaryn-tab-btn${tab === "installed" ? " locaryn-active" : ""}`}
            onClick={() => setTab("installed")}
          >
            Installées ({installed.length})
          </button>
          <button
            type="button"
            className={`locaryn-tab-btn${tab === "browse" ? " locaryn-active" : ""}`}
            onClick={() => setTab("browse")}
          >
            Découvrir
          </button>
        </div>

        <div style={{ display: "flex", gap: 8 }}>
          <button
            type="button"
            className="locaryn-btn-ghost"
            style={{ fontSize: 12 }}
            disabled={updatingAll || busy !== null || pendingCount === 0}
            title={
              updatingAll
                ? "Mise à jour en cours…"
                : "Réinstalle depuis sa source chaque extension non à jour — installée ou vue dans le catalogue."
            }
            onClick={updateAll}
          >
            {updatingAll
              ? "Mise à jour…"
              : `Tout mettre à jour${pendingCount > 0 ? ` (${pendingCount})` : ""}`}
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            style={{ fontSize: 12 }}
            onClick={() => setSourcesOpen((v) => !v)}
          >
            Sources ({sources.filter((s) => s.enabled).length})
          </button>
          <button
            type="button"
            className="locaryn-btn-primary"
            style={{ fontSize: 12, padding: "4px 12px" }}
            onClick={() => setInstallDialog({ open: true, kind: "extension" })}
          >
            + Depuis un dépôt GitHub
          </button>
        </div>
      </div>

      {batchProgress && (
        <div className="locaryn-card" style={{ marginBottom: 12, padding: "10px 14px" }}>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              gap: 12,
            }}
          >
            <p className="locaryn-field-hint" style={{ margin: 0 }}>
              {batchProgress.done}/{batchProgress.total} —{" "}
              {batchProgress.current.length > 0
                ? `en cours : ${batchProgress.current.join(", ")}`
                : "en attente…"}
            </p>
            <button
              type="button"
              className="locaryn-btn-ghost"
              style={{ fontSize: 12, flexShrink: 0 }}
              onClick={cancelBatch}
            >
              Annuler
            </button>
          </div>
          <div
            style={{
              height: 4,
              borderRadius: 2,
              background: "var(--border)",
              overflow: "hidden",
              marginTop: 8,
            }}
          >
            <div
              style={{
                height: "100%",
                width: `${((batchProgress.done + batchProgress.current.length) / batchProgress.total) * 100}%`,
                background: "var(--accent)",
                transition: "width .2s ease",
              }}
            />
          </div>
        </div>
      )}

      {batchReport && (
        <div className="locaryn-card" style={{ marginBottom: 12, padding: "12px 14px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <strong style={{ fontSize: 13 }}>
              {batchReport.cancelled ? "Mise à jour en lot annulée" : "Mise à jour en lot terminée"}
            </strong>
            <button
              type="button"
              className="locaryn-btn-ghost"
              style={{ fontSize: 12 }}
              onClick={() => setBatchReport(null)}
            >
              Fermer
            </button>
          </div>
          <p className="locaryn-field-hint" style={{ marginTop: 6 }}>
            {batchReport.updated.length === 0
              ? "Aucune mise à jour"
              : `${batchReport.updated.length} mise${batchReport.updated.length > 1 ? "s" : ""} à jour`}
            {batchReport.skipped > 0 ? `, ${batchReport.skipped} déjà à jour` : ""}
            {batchReport.failed.length > 0
              ? `, ${batchReport.failed.length} échec${batchReport.failed.length > 1 ? "s" : ""}`
              : ""}
            {batchReport.cancelled ? ", arrêtée sur demande." : "."}
          </p>
          {batchReport.updated.length > 0 && (
            <p className="locaryn-field-hint" style={{ marginTop: 4 }}>
              À jour : {batchReport.updated.join(", ")}
            </p>
          )}
          {batchReport.failed.length > 0 && (
            <ul style={{ margin: "8px 0 0", paddingLeft: 18, color: "var(--danger)" }}>
              {batchReport.failed.map((f) => (
                <li key={f.name} style={{ fontSize: 12, marginBottom: 4 }}>
                  {f.name} : {f.reason}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {error && (
        <p className="locaryn-field-hint" style={{ color: "var(--danger)", marginBottom: 12 }}>
          {error}
        </p>
      )}
      {notice && (
        <p className="locaryn-field-hint" style={{ marginBottom: 12 }}>
          {notice}
        </p>
      )}

      {sourcesOpen && (
        <div className="locaryn-card" style={{ marginBottom: 20, padding: 16 }}>
          <h4 style={{ fontSize: "var(--text-md)", marginBottom: 4 }}>Sources de catalogue</h4>
          <p className="locaryn-field-hint" style={{ marginBottom: 12 }}>
            Les marketplaces Claude Code sont des dépôts contenant{" "}
            <code>.claude-plugin/marketplace.json</code>. Ajoutez-en une par <code>owner/repo</code>
            .
          </p>
          {sources.map((s) => {
            const st = snapshot?.sources.find((x) => x.source.id === s.id);
            return (
              <div
                key={s.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 10,
                  padding: "8px 0",
                  borderBottom: "1px solid var(--border)",
                }}
              >
                <div style={{ minWidth: 0 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <span
                      className={`locaryn-health-dot ${
                        st ? (st.ok ? "locaryn-health-ok" : "locaryn-health-off") : ""
                      }`}
                    />
                    <strong style={{ fontSize: 13 }}>{s.label}</strong>
                    <span className="locaryn-tag">{ECOSYSTEM_LABELS[s.ecosystem]}</span>
                  </div>
                  <span
                    className="locaryn-field-hint"
                    style={{ display: "block", wordBreak: "break-all" }}
                  >
                    {st?.error ?? (st ? `${st.entry_count} entrées` : s.url)}
                  </span>
                </div>
                <div style={{ display: "flex", gap: 6, flexShrink: 0 }}>
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{ fontSize: 12 }}
                    onClick={() => core.setCatalogSourceEnabled(s.id, !s.enabled).then(setSources)}
                  >
                    {s.enabled ? "Désactiver" : "Activer"}
                  </button>
                  {!s.builtin && (
                    <button
                      type="button"
                      className="locaryn-btn-ghost"
                      style={{ fontSize: 12, color: "var(--danger)" }}
                      onClick={() => core.removeCatalogSource(s.id).then(setSources)}
                    >
                      Retirer
                    </button>
                  )}
                </div>
              </div>
            );
          })}
          <button
            type="button"
            className="locaryn-btn-ghost"
            style={{ fontSize: 12, marginTop: 12 }}
            onClick={() => setInstallDialog({ open: true, kind: "marketplace" })}
          >
            + Ajouter une marketplace
          </button>
        </div>
      )}

      {tab === "installed" ? (
        installed.length === 0 ? (
          <p className="locaryn-field-hint">
            Aucune extension installée. Ouvrez « Découvrir » pour parcourir les catalogues Claude
            Code, Gemini CLI, OpenCode et MCP, ou installez directement depuis un dépôt GitHub.
          </p>
        ) : (
          grouped.map(([eco, list]) => (
            <div key={eco} style={{ marginBottom: 24 }}>
              <h4 style={{ fontSize: "var(--text-md)", marginBottom: 10 }}>
                {ECOSYSTEM_LABELS[eco]}
              </h4>
              <div className="locaryn-model-grid">
                {list.map((e) => (
                  <div key={e.id} className="locaryn-box-card">
                    <div className="locaryn-box-head">
                      <div>
                        <h3 className="locaryn-box-name">{e.name}</h3>
                        <span className="locaryn-box-brand">
                          v{e.version}
                          {e.author ? ` · ${e.author}` : ""}
                        </span>
                      </div>
                      <span className={`locaryn-tag${e.enabled ? " locaryn-tag-installed" : ""}`}>
                        {e.enabled ? "actif" : "inactif"}
                      </span>
                      {updates[e.id]?.update_available && (
                        <span
                          className="locaryn-tag"
                          style={{ color: "var(--accent)" }}
                          title={`v${updates[e.id]?.latest_version ?? ""} disponible sur la branche par défaut du dépôt`}
                        >
                          Mise à jour dispo · v{updates[e.id]?.latest_version}
                        </span>
                      )}
                    </div>

                    <p className="locaryn-box-desc">
                      {e.description ?? "Pas de description fournie."}
                    </p>
                    <p className="locaryn-field-hint">{componentSummary(e)}</p>
                    {updates[e.id]?.error && (
                      <p
                        className="locaryn-field-hint"
                        style={{ color: "var(--text-faint)", marginTop: 4 }}
                      >
                        Vérification de mise à jour impossible : {updates[e.id]?.error}
                      </p>
                    )}

                    {e.permissions.length > 0 && (
                      <p className="locaryn-field-hint" style={{ marginTop: 6 }}>
                        Permissions :{" "}
                        {e.permissions
                          .map(
                            (p) =>
                              `${PERMISSION_LABELS[p.permission]}${p.granted ? "" : " (refusée)"}`,
                          )
                          .join(", ")}
                      </p>
                    )}

                    {e.load_errors.length > 0 && (
                      <p
                        className="locaryn-field-hint"
                        style={{ color: "var(--danger)", marginTop: 6 }}
                      >
                        {e.load_errors.length} composant(s) illisible(s) : {e.load_errors[0]}
                      </p>
                    )}

                    <div
                      style={{
                        marginTop: "auto",
                        paddingTop: 12,
                        borderTop: "1px solid var(--border)",
                        display: "flex",
                        justifyContent: "space-between",
                        gap: 8,
                      }}
                    >
                      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                        <button
                          type="button"
                          className="locaryn-btn-ghost"
                          style={{ fontSize: 12 }}
                          disabled={updatingAll || busy === e.id || !e.source}
                          title={
                            e.source
                              ? "Réinstalle depuis la même source (github:…, dossier ou zip)"
                              : "Aucune source enregistrée — impossible de mettre à jour."
                          }
                          onClick={() => update(e)}
                        >
                          {busy === e.id ? "…" : "Mettre à jour"}
                        </button>
                        <button
                          type="button"
                          className="locaryn-btn-ghost"
                          style={{ fontSize: 12 }}
                          disabled={updatingAll}
                          onClick={() => setConfiguring(e)}
                        >
                          Régler
                        </button>
                        <button
                          type="button"
                          className="locaryn-btn-ghost"
                          style={{ fontSize: 12 }}
                          onClick={() =>
                            setPermissionExt({
                              ext: e,
                              grants: new Set(
                                e.permissions.filter((p) => p.granted).map((p) => p.permission),
                              ),
                              ctx: "edit",
                            })
                          }
                          disabled={updatingAll || e.permissions.length === 0}
                        >
                          Permissions
                        </button>
                      </div>
                      <div style={{ display: "flex", gap: 8 }}>
                        <button
                          type="button"
                          className={e.enabled ? "locaryn-btn-ghost" : "locaryn-btn-primary"}
                          style={{ fontSize: 12 }}
                          disabled={updatingAll || busy === e.id}
                          onClick={() => toggleEnabled(e)}
                        >
                          {busy === e.id ? "…" : e.enabled ? "Désactiver" : "Activer"}
                        </button>
                        <button
                          type="button"
                          className="locaryn-btn-ghost"
                          style={{ fontSize: 12, color: "var(--danger)" }}
                          disabled={updatingAll || busy === e.id}
                          onClick={() => remove(e)}
                        >
                          Désinstaller
                        </button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ))
        )
      ) : (
        <>
          <div style={{ display: "flex", gap: 8, marginBottom: 12, flexWrap: "wrap" }}>
            <input
              className="locaryn-input"
              style={{ flex: "1 1 220px" }}
              placeholder="Rechercher une extension…"
              value={query}
              onChange={(ev) => setQuery(ev.target.value)}
            />
            <button
              type="button"
              className="locaryn-btn-ghost"
              disabled={refreshing}
              onClick={refreshCatalog}
            >
              {refreshing ? "Actualisation…" : "Actualiser"}
            </button>
          </div>

          <div className="locaryn-size-chips" style={{ marginBottom: 16 }}>
            {ECOSYSTEM_FILTERS.map((f) => (
              <button
                key={f.id}
                type="button"
                className={`locaryn-chip${ecosystem === f.id ? " locaryn-chip-on" : ""}`}
                onClick={() => setEcosystem(f.id)}
              >
                {f.label}
              </button>
            ))}
          </div>

          {snapshot?.fetched_at == null ? (
            <p className="locaryn-field-hint">
              Aucun catalogue en cache. Lancez « Actualiser » pour lire les marketplaces Claude
              Code, l'index Gemini CLI, le registre MCP officiel et les plugins OpenCode publiés sur
              npm.
            </p>
          ) : entries.length === 0 ? (
            <p className="locaryn-field-hint">Aucun résultat pour cette recherche.</p>
          ) : (
            <div className="locaryn-model-grid">
              {entries.map((c: CatalogEntry) => {
                const compat = COMPAT[c.compat] ?? COMPAT.unsupported;
                const canInstall = c.compat !== "unsupported" && !!c.install_source;
                return (
                  <div key={c.id} className="locaryn-box-card">
                    <div className="locaryn-box-head">
                      <div style={{ minWidth: 0 }}>
                        <h3 className="locaryn-box-name">{c.display_name}</h3>
                        <span className="locaryn-box-brand">{c.catalog_label}</span>
                      </div>
                      <span className="locaryn-tag">{ECOSYSTEM_LABELS[c.ecosystem]}</span>
                    </div>

                    <p className="locaryn-box-desc">
                      {c.description ?? "Pas de description fournie."}
                    </p>

                    <p className="locaryn-field-hint" title={compat.hint}>
                      {compat.label}
                      {c.advertised.length > 0 && ` · ${c.advertised.join(" · ")}`}
                    </p>

                    <div
                      style={{
                        marginTop: "auto",
                        paddingTop: 12,
                        borderTop: "1px solid var(--border)",
                        display: "flex",
                        justifyContent: "flex-end",
                      }}
                    >
                      {c.installed ? (
                        <span className="locaryn-tag locaryn-tag-installed">installée</span>
                      ) : (
                        <button
                          type="button"
                          className="locaryn-btn-primary"
                          style={{ fontSize: 12 }}
                          disabled={!canInstall || busy === c.id || updatingAll}
                          title={canInstall ? c.install_source : compat.hint}
                          onClick={() => installFromCatalog(c)}
                        >
                          {busy === c.id ? "Installation…" : "Installer"}
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </>
      )}

      {installDialog.open && (
        <ExtensionInstallDialog
          kind={installDialog.kind}
          initialSource={installDialog.initialSource}
          onClose={() => setInstallDialog((s) => ({ ...s, open: false }))}
          onExtensionInstalled={(ext, enable) => finishInstall(ext, enable)}
          onMarketplaceAdded={(sources) => {
            setSources(sources);
            setNotice("Marketplace ajoutée. Lancez « Actualiser » pour la lire.");
          }}
        />
      )}

      {configuring && (
        <ExtensionConfigPanel
          extension={configuring}
          onClose={() => {
            setConfiguring(null);
            loadInstalled();
          }}
        />
      )}

      {permissionExt && (
        <ExtensionPermissionsModal
          extension={permissionExt.ext}
          initialGrants={permissionExt.grants}
          onDone={handlePermissionsDone}
        />
      )}
    </div>
  );
}
