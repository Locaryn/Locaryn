import { Icon, LoProgress } from "@locaryn/ui-core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { BatchStudio } from "./components/BatchStudio";
import { ChatPermissionsModal } from "./components/ChatPermissionsModal";
import { ConnectScreen } from "./components/ConnectScreen";
import { ConnectorsSettings } from "./components/ConnectorsSettings";
import { ExtensionsSettings } from "./components/ExtensionsSettings";
import { ModelBrowser } from "./components/ModelBrowser";
import { ModelResidency } from "./components/ModelResidency";
import { CAPABILITY_GATED_VIEWS, NAVIGABLE_VIEWS, NavDrawer } from "./components/NavDrawer";
import { ProjectSettingsModal } from "./components/ProjectSettingsModal";
import { SettingsPanel } from "./components/SettingsPanel";
import { RunningTask, TaskCenter } from "./components/TaskCenter";
import { TopBar } from "./components/TopBar";
import { UpdateDialog } from "./components/UpdateDialog";
import { ExtensionScreen } from "./components/extensions/ExtensionScreen";
import { useTheme } from "./hooks/useTheme";
import { FREE_CHAT_PATH } from "./lib/constants";
import {
  type Health,
  type HfModelSelection,
  type InstalledExtension,
  type Project,
  type Provisioning,
  type Session,
  core,
  coreMode,
} from "./lib/core";
import { parseInstallLink, setPendingInstall } from "./lib/deepLink";
import { pickFolder } from "./lib/dialog";
import type { ModelDownloadSource } from "./lib/modelRegistry";
import { setRunReveal } from "./lib/runPanel";
import { taskCenter } from "./lib/taskCenter";
import { BottomPanel } from "./panels/BottomPanel";
import { ChatPanel } from "./panels/ChatPanel";
import { LeftPanel } from "./panels/LeftPanel";
import { ModelConfigPanel } from "./panels/ModelConfigPanel";
import { RunPanel } from "./panels/RunPanel";
import { AccountView } from "./views/AccountView";
import { FiguresView } from "./views/FiguresView";
import { InstalledModelsView } from "./views/InstalledModelsView";
import { ModelStudioView } from "./views/ModelStudioView";
import { type Section, SettingsView } from "./views/SettingsView";
import { StudioView } from "./views/StudioView";

/** Les trois séparations déplaçables de la fenêtre. */
type PanelKey = "leftW" | "rightW" | "bottomH";

/** Bornes de chaque panneau. Elles existent pour que la fenêtre reste
 *  utilisable : sans minimum, une poignée tirée à fond fait disparaître le
 *  panneau sans moyen de le rattraper ; sans maximum, elle écrase le chat. */
const PANEL_LIMITS: Record<PanelKey, { min: number; max: number }> = {
  leftW: { min: 160, max: 500 },
  rightW: { min: 240, max: 640 },
  bottomH: { min: 120, max: 520 },
};

function clampPanel(panel: PanelKey, value: number): number {
  const { min, max } = PANEL_LIMITS[panel];
  return Math.max(min, Math.min(max, Math.round(value)));
}

/** Le nom de l'ecran ouvert en surcouche, annonce aux lecteurs d'ecran. */
const OVERLAY_LABELS: Record<string, string> = {
  models: "Marketplace",
  installed: "Modèles installés",
  settings: "Réglages",
  account: "Compte et profil",
  extensions: "Morphs et Skills",
  connectors: "Connecteurs et MCP",
  training: "Studio d'entraînement",
  batch: "Traitement par lots",
  figures: "Figures",
  studio: "Studio",
};

export function App() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [activeProject, setActiveProject] = useState<Project | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  /** Toutes les conversations, indexées par projet, pour l'historique groupé. */
  const [sessionsByProject, setSessionsByProject] = useState<Record<string, Session[]>>({});
  const [standaloneSessions, setStandaloneSessions] = useState<Session[]>([]);
  /** Hidden project that owns free (project-less) chats. */
  const [freeProject, setFreeProject] = useState<Project | null>(null);
  const [activeSession, setActiveSession] = useState<Session | null>(null);
  const [health, setHealth] = useState<Health | null>(null);
  const [installedModels, setInstalledModels] = useState<string[]>([]);
  /**
   * Ce que les extensions actives savent faire.
   *
   * Décide de la présence d'écrans entiers : le Studio de génération n'existe
   * que si une extension apporte `image-gen`, `voice-tts`… et disparaît quand
   * la dernière est retirée ou désactivée. Les badges de capacité du
   * catalogue de modèles suivent la même liste.
   */
  const [activeCapabilities, setActiveCapabilities] = useState<string[]>([]);
  /** Ignore an older extension scan if a newer install/remove event finishes first. */
  const capabilitiesRefreshId = useRef(0);
  /** Noyaux alternatifs installés (extensions avec une section `core`). */
  const [installedCores, setInstalledCores] = useState<InstalledExtension[]>([]);
  /** Les extensions actives : la navigation et le Studio en tirent leurs
   *  contributions (nav_items, studio_tabs) sans jamais les nommer. */
  const [activeExtensions, setActiveExtensions] = useState<InstalledExtension[]>([]);
  /** Projet pour lequel le choix du noyau est ouvert (création de session). */
  const [corePickerFor, setCorePickerFor] = useState<Project | null>(null);

  const theme = useTheme();

  const [downloadProgress, setDownloadProgress] = useState<{
    tag: string;
    progress: number;
    status?: string;
  } | null>(null);

  // Active top-level view: "chat" | "models" | "studio" | "training" | "connectors" | "settings" | "account"
  const [activeView, setActiveView] = useState<string>("chat");
  const [settingsInitialSection, setSettingsInitialSection] = useState<Section | undefined>(
    undefined,
  );

  // Toggleable panels & drawers
  const [leftOpen, setLeftOpen] = useState(true);
  const [showModelConfig, setShowModelConfig] = useState(false);
  const [showPreview, setShowPreview] = useState(false);
  // A run opens the pane that displays it, so output is never produced into a
  // panel the user cannot see.
  useEffect(() => {
    setRunReveal(() => setShowPreview(true));
    return () => setRunReveal(null);
  }, []);
  const [showBottom, setShowBottom] = useState(false);
  /** Chat governance dialog (the shield button in the top bar). */
  const [permissionsOpen, setPermissionsOpen] = useState(false);
  /** Project whose settings dialog is open (from the sidebar menu). */
  const [projectSettings, setProjectSettings] = useState<Project | null>(null);

  /**
   * Whether this installation must sign in before the application is usable.
   *
   * "checking" while we look, so nothing flashes on screen; "connect" only
   * when an administrator prepared this machine for a server and no session
   * exists yet. A personal installation never sees any of it.
   */
  const [gate, setGate] = useState<"checking" | "connect" | "ready">("checking");
  const [provisioning, setProvisioning] = useState<Provisioning | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [prov, session] = await Promise.all([core.provisioning(), core.currentSession()]);
        if (cancelled) return;
        setProvisioning(prov);
        setGate(prov && !session ? "connect" : "ready");
      } catch {
        // A deployment file that cannot be read must not lock anyone out of
        // their own machine.
        if (!cancelled) setGate("ready");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Tailles des panneaux redimensionnables, en pixels.
  const [leftW, setLeftW] = useState(260);
  const [rightW, setRightW] = useState(340);
  const [bottomH, setBottomH] = useState(220);

  // Resizing state
  const isDragging = useRef<string | null>(null);

  useEffect(() => {
    bootstrap();
  }, []);

  /**
   * Relit ce que les extensions actives apportent.
   *
   * Appelée au démarrage et après chaque installation, activation ou retrait :
   * c'est ce qui fait apparaître le Studio quand on ajoute la génération
   * d'images, et disparaître quand on la retire.
   */
  const refreshCapabilities = useCallback(async () => {
    const refreshId = ++capabilitiesRefreshId.current;
    try {
      const installed = await core.listExtensions();
      if (refreshId !== capabilitiesRefreshId.current) return;
      const caps = new Set<string>();
      for (const ext of installed) {
        if (!ext.enabled) continue;
        for (const c of ext.capabilities ?? []) caps.add(c);
      }
      setActiveCapabilities([...caps]);
      // Un tunnel est une capacité du plugin Remote, pas du socle local. Si le
      // plugin a été désactivé ou retiré, on coupe aussi un tunnel qui aurait
      // été laissé actif par la session précédente.
      if (!caps.has("travel-tunnel")) {
        void core.setTravelMode(null).catch(() => {});
      }
      // Les noyaux installés alimentent le choix à la création de session.
      setInstalledCores(installed.filter((e) => e.core != null && e.enabled));
      setActiveExtensions(installed.filter((e) => e.enabled));
    } catch {
      if (refreshId !== capabilitiesRefreshId.current) return;
      // Registre illisible : on n'invente pas de capacités. L'interface se
      // réduit à ce qui marche sans extension.
      setActiveCapabilities([]);
      setInstalledCores([]);
      setActiveExtensions([]);
    }
  }, []);

  useEffect(() => {
    void refreshCapabilities();
    // L'écran des extensions émet cet évènement après chaque changement, pour
    // que la navigation suive sans qu'on ait à redémarrer l'application.
    const onChange = () => void refreshCapabilities();
    window.addEventListener("locaryn:extensions-changed", onChange);
    return () => window.removeEventListener("locaryn:extensions-changed", onChange);
  }, [refreshCapabilities]);

  // Une extension peut demander l'ouverture d'un écran de l'hôte plutôt que
  // de recopier chez elle ce que l'application sait déjà faire. Le nom de la
  // vue est le seul argument : l'hôte ne fournit rien d'autre, et une vue
  // inconnue ne fait rien.
  useEffect(() => {
    const onNavigate = (event: Event) => {
      const view = (event as CustomEvent<{ view?: unknown }>).detail?.view;
      if (typeof view === "string" && NAVIGABLE_VIEWS.includes(view)) setActiveView(view);
    };
    window.addEventListener("locaryn:action:navigate", onNavigate);
    return () => window.removeEventListener("locaryn:action:navigate", onNavigate);
  }, []);

  // Retirer l'extension qui portait l'écran ouvert laisserait la personne
  // devant une vue qui n'existe plus. On revient au chat plutôt que d'afficher
  // le vide.
  // Echap ferme l'ecran ouvert par-dessus le chat.
  //
  // Pose sur la fenetre plutot que sur le panneau : le panneau contient des
  // champs et des listes qui prennent le focus, et l'evenement ne remonterait
  // pas jusqu'a lui depuis un menu natif.
  useEffect(() => {
    if (activeView === "chat") return;
    const surTouche = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // Un tiroir ouvert par-dessus l'ecran se ferme en premier : sinon Echap
      // emporte le panneau entier et on repart au chat alors qu'on voulait
      // seulement refermer le detail qu'on venait d'ouvrir.
      if (document.querySelector(".locaryn-drawer-layer")) return;
      setActiveView("chat");
    };
    window.addEventListener("keydown", surTouche);
    return () => window.removeEventListener("keydown", surTouche);
  }, [activeView]);

  useEffect(() => {
    const needs = CAPABILITY_GATED_VIEWS[activeView];
    if (needs && !needs.some((c) => activeCapabilities.includes(c))) {
      setActiveView("chat");
    }
  }, [activeView, activeCapabilities]);

  // Deep links (`locaryn://install?src=owner/repo`): a link can open the app
  // from a cold start (URL passed as CLI argument — read via `get_current`)
  // or land while it is already running (forwarded by the plugin as an event,
  // and re-emitted by Rust as `locaryn://deep-link`). Either way: remember the
  // intent and open the settings panel; the extensions section picks it up
  // when it mounts and pre-fills the install dialog.
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    const handleUrl = (url: string) => {
      const intent = parseInstallLink(url);
      if (!intent) return;
      setPendingInstall(intent);
      setActiveView("settings");
    };

    if (coreMode === "tauri") {
      // Lien qui a ouvert l'app (démarrage à froid).
      core
        .pendingDeepLink()
        .then((url) => {
          if (url) handleUrl(url);
        })
        .catch(() => {});
      // Liens reçus pendant que l'app tourne : événement natif du plugin puis
      // re-émission Rust (une seule suffit, les deux ne se gênent pas).
      listen<string[]>("deep-link://new-url", (ev) => {
        ev.payload?.forEach(handleUrl);
      })
        .then((un) => unlisteners.push(un))
        .catch(() => {});
      listen<string>("locaryn://deep-link", (ev) => handleUrl(ev.payload))
        .then((un) => unlisteners.push(un))
        .catch(() => {});
    } else {
      // En démo (navigateur), on simule un lien par ancre : #locaryn://install?src=…
      const fromHash = () => {
        const h = window.location.hash.replace(/^#/, "");
        if (h) handleUrl(h);
      };
      fromHash();
      window.addEventListener("hashchange", fromHash);
      unlisteners.push(() => window.removeEventListener("hashchange", fromHash));
    }

    return () => {
      for (const un of unlisteners) un();
    };
  }, []);

  async function bootstrap() {
    try {
      const b = await core.bootstrap();
      setActiveProject(b.project);
      // Start on the home screen rather than inside a chat: the first prompt
      // creates (and auto-names) the conversation.
      setActiveSession(null);
      setHealth(b.health);

      // Free chats live in a hidden internal project so they never show up
      // inside a real one. Keep it out of the sidebar's project list.
      const free = await core.freeChatProject().catch(() => null);
      setFreeProject(free);

      const projs = await core.listProjects();
      const realProjects = projs.filter((p) => p.id !== free?.id && p.path !== FREE_CHAT_PATH);
      setProjects(realProjects);

      // Charger chaque projet dès le départ : l'historique reste organisé comme
      // un arbre de travail, même quand un autre projet est ouvert au centre.
      const groupedEntries = await Promise.all(
        realProjects.map(
          async (p) => [p.id, await core.listSessions(p.id).catch(() => [])] as const,
        ),
      );
      const groupedSessions = Object.fromEntries(groupedEntries) as Record<string, Session[]>;
      setSessionsByProject(groupedSessions);

      const initialProject =
        b.project && b.project.id !== free?.id ? b.project : (realProjects[0] ?? null);
      setActiveProject(initialProject);
      setSessions(initialProject ? (groupedSessions[initialProject.id] ?? []) : []);

      if (free) {
        setStandaloneSessions(await core.listSessions(free.id).catch(() => []));
      }

      await refreshInstalledModels();
    } catch (e) {
      console.warn("Bootstrap fallback:", e);
    }
  }

  async function refreshInstalledModels() {
    try {
      const p = await core.listProviders();
      const active = p.find((pr) => pr.is_active) ?? p[0];
      const chatModels = active ? await core.listModels(active.endpoint).catch(() => []) : [];
      setInstalledModels(chatModels);
    } catch {
      setInstalledModels([]);
    }
  }

  async function refreshHealth() {
    try {
      const h = await core.health();
      setHealth(h);
      await refreshInstalledModels();
    } catch (e) {
      console.warn("Health check failed:", e);
    }
  }

  async function handleSelectProject(proj: Project | null) {
    setActiveProject(proj);
    if (!proj) {
      setSessions([]);
      return;
    }
    try {
      const sList = await core.listSessions(proj.id);
      setSessionsByProject((prev) => ({ ...prev, [proj.id]: sList }));
      setSessions(sList);
      if (sList.length > 0) {
        setActiveSession(sList[0]);
      } else {
        const newS = await core.createSession(proj.id);
        setSessionsByProject((prev) => ({ ...prev, [proj.id]: [newS] }));
        setSessions([newS]);
        setActiveSession(newS);
      }
    } catch (e) {
      console.warn("Select project failed:", e);
    }
  }

  async function handleSelectSession(session: Session) {
    const project = projects.find((p) => p.id === session.project_id);
    if (project) {
      setActiveProject(project);
      setSessions(sessionsByProject[project.id] ?? []);
    } else if (session.project_id === freeProject?.id) {
      setActiveProject(null);
      setSessions([]);
    }
    setActiveSession(session);
  }

  async function handleNewSession(proj: Project) {
    // Des noyaux alternatifs sont installés : on demande lequel pilotera la
    // conversation. Le noyau Locaryn reste le défaut, en tête de liste.
    if (installedCores.length > 0) {
      setCorePickerFor(proj);
      return;
    }
    await createSessionWithCore(proj, null);
  }

  /** Créer une session, noyau Locaryn (`coreId = null`) ou alternatif. */
  async function createSessionWithCore(proj: Project, coreId: string | null) {
    try {
      const newS = await core.createSession(proj.id, undefined, coreId);
      setSessionsByProject((prev) => ({
        ...prev,
        [proj.id]: [newS, ...(prev[proj.id] ?? [])],
      }));
      setActiveProject(proj);
      setSessions((prev) => (activeProject?.id === proj.id ? [newS, ...prev] : [newS]));
      setActiveSession(newS);
      setActiveView("chat");
    } catch (e) {
      console.warn("Create session failed:", e);
    }
  }

  /**
   * Ranger une conversation aux archives.
   *
   * Elle sort des listes, rien n'est effacé. C'est le geste courant : on
   * range, on ne détruit pas.
   */
  async function handleArchiveSession(s: Session) {
    try {
      await core.archiveSession(s.id, true);
    } catch (e) {
      console.error(e);
      return;
    }
    setStandaloneSessions((prev) => prev.filter((x) => x.id !== s.id));
    setSessionsByProject((prev) => ({
      ...prev,
      [s.project_id]: (prev[s.project_id] ?? []).filter((x) => x.id !== s.id),
    }));
    setSessions((prev) => prev.filter((x) => x.id !== s.id));
    if (activeSession?.id === s.id) setActiveSession(null);
  }

  /** Déplacer une conversation dans un projet. */
  async function handleMoveSession(s: Session, projectId: string) {
    try {
      await core.moveSession(s.id, projectId);
    } catch (e) {
      console.error(e);
      return;
    }
    const moved = { ...s, project_id: projectId };
    setStandaloneSessions((prev) => prev.filter((x) => x.id !== s.id));
    setSessionsByProject((prev) => ({
      ...prev,
      [s.project_id]: (prev[s.project_id] ?? []).filter((x) => x.id !== s.id),
      [projectId]: [moved, ...(prev[projectId] ?? []).filter((x) => x.id !== s.id)],
    }));
    setSessions((prev) => prev.filter((x) => x.id !== s.id));
    if (activeProject?.id === projectId) {
      setSessions((prev) => [moved, ...prev]);
    }
  }

  /**
   * Verser une conversation dans une autre.
   *
   * Le petit modèle relit les deux fils et en écrit un seul récit, ajouté à
   * la conversation d'accueil. Celle qui a été déposée part aux archives : si
   * le résumé a perdu quelque chose, elle est encore là pour le dire.
   *
   * Le travail prend du temps — un modèle relit deux conversations entières —
   * donc il passe par le centre de notifications plutôt que de laisser la
   * barre latérale figée sans explication.
   */
  async function handleMergeSessions(accueil: Session, sourceId: string) {
    const tache = taskCenter.add({
      type: "workflow",
      label: `Réunion dans « ${accueil.title ?? "cette conversation"} »`,
      detail: "Le petit modèle relit les deux fils",
    });
    try {
      await core.mergeSessions(accueil.id, sourceId);
      taskCenter.done(tache, { detail: "Conversations réunies" });
      setStandaloneSessions((prev) => prev.filter((x) => x.id !== sourceId));
      setSessionsByProject((prev) =>
        Object.fromEntries(
          Object.entries(prev).map(([projectId, list]) => [
            projectId,
            list.filter((x) => x.id !== sourceId),
          ]),
        ),
      );
      setSessions((prev) => prev.filter((x) => x.id !== sourceId));
      // Rouvrir celle d'accueil : le récit vient d'y être écrit.
      if (activeSession?.id === accueil.id) handleSelectSession(accueil);
    } catch (e) {
      taskCenter.fail(tache, String(e));
    }
  }

  /** Renommer à la main : le titre devient définitif. */
  async function handleRenameSession(s: Session, title: string) {
    try {
      await core.renameSession(s.id, title);
    } catch (e) {
      console.error(e);
      return;
    }
    const rename = (list: Session[]) => list.map((x) => (x.id === s.id ? { ...x, title } : x));
    setStandaloneSessions(rename);
    setSessionsByProject((prev) =>
      Object.fromEntries(Object.entries(prev).map(([id, list]) => [id, rename(list)])),
    );
    setSessions(rename);
    if (activeSession?.id === s.id) setActiveSession({ ...activeSession, title });
  }

  /** Une conversation dont rien ne sera gardé. */
  async function handleNewEphemeralChat() {
    try {
      const project = freeProject ?? (await core.freeChatProject());
      if (!freeProject) setFreeProject(project);
      const s = await core.createEphemeralSession(project.id);
      setActiveProject(null);
      setActiveSession(s);
      setActiveView("chat");
    } catch (e) {
      console.error(e);
    }
  }

  function handleNewStandaloneChat() {
    // Open a fresh, unsaved chat view. The session is only created when the
    // user sends the first message, and the LLM picks a title from context.
    setActiveProject(null);
    setActiveSession(null);
    setActiveView("chat");
  }

  /** Home screen → first prompt creates the chat. The LLM chooses the title
   *  from the project context; until then it appears as untitled. */
  async function handleCreateSessionForPrompt(
    firstPrompt: string,
    targetProjectId?: string | null,
  ): Promise<Session | null> {
    try {
      // Chats started from the home screen belong to the target or active project when
      // there is one, otherwise to the hidden free-chat project.
      const chosenProject = targetProjectId
        ? (projects.find((p) => p.id === targetProjectId) ?? null)
        : activeProject;

      let project = chosenProject;
      if (!project) {
        project = freeProject ?? (await core.freeChatProject());
        if (!freeProject) setFreeProject(project);
      }
      const s = await core.createSession(project.id);
      if (!chosenProject) {
        setStandaloneSessions((prev) => [s, ...prev]);
      } else {
        setSessionsByProject((prev) => ({
          ...prev,
          [project.id]: [s, ...(prev[project.id] ?? [])],
        }));
        if (activeProject?.id === project.id) {
          setSessions((prev) => [s, ...prev]);
        }
      }
      setActiveSession(s);

      // Ask the LLM for a concise title in the background.
      core
        .generateSessionTitle(s.id, firstPrompt)
        .then((title) => {
          const newS = { ...s, title };
          if (!chosenProject) {
            setStandaloneSessions((prev) => prev.map((x) => (x.id === s.id ? newS : x)));
          } else {
            setSessionsByProject((prev) => ({
              ...prev,
              [project.id]: (prev[project.id] ?? []).map((x) => (x.id === s.id ? newS : x)),
            }));
            if (activeProject?.id === project.id) {
              setSessions((prev) => prev.map((x) => (x.id === s.id ? newS : x)));
            }
          }
          setActiveSession((current) => (current?.id === s.id ? newS : current));
        })
        .catch((e) => {
          console.warn("Title generation failed:", e);
        });

      return s;
    } catch (e) {
      console.warn("Create session from prompt failed:", e);
      return null;
    }
  }

  async function handleAddProject(path: string, name: string) {
    try {
      const newP = await core.createProject(path, name);
      setProjects((prev) => [...prev, newP]);
      await handleSelectProject(newP);
    } catch (e) {
      console.warn("Add project failed:", e);
    }
  }

  async function handleInstallModel(
    tag: string,
    onProgress?: (pct: number) => void,
    heretic?: boolean,
    consent?: boolean,
    selection?: HfModelSelection,
    downloads?: ModelDownloadSource[],
  ) {
    const p = await core.listProviders();
    const active = p.find((pr) => pr.is_active) ?? p[0];
    if (!active) return;

    setDownloadProgress({ tag, progress: 0, status: "Démarrage du téléchargement..." });
    const shortName = tag.split("/").pop() || tag;
    const taskId = taskCenter.add({
      type: "download",
      label: `Téléchargement : ${shortName}${heretic ? " (sans limite)" : ""}`,
      progress: 0,
    });

    try {
      await core.pullModel(
        active.endpoint,
        tag,
        (pct, statusText) => {
          onProgress?.(pct);
          setDownloadProgress({ tag, progress: pct, status: statusText });
          taskCenter.update(taskId, { progress: pct, detail: statusText ?? `${pct}%` });
        },
        heretic,
        consent,
        selection,
        downloads,
      );
      setDownloadProgress({ tag, progress: 100, status: "Téléchargement terminé" });
      taskCenter.done(taskId);
      setTimeout(() => setDownloadProgress(null), 3000);
    } catch (e) {
      console.error("Pull failed", e);
      setDownloadProgress(null);
      taskCenter.fail(taskId, String(e).replace(/^Error:\s*/, ""));
    } finally {
      await refreshHealth();
    }
  }

  async function handleCancelDownload() {
    try {
      await core.cancelPullModel();
      setDownloadProgress(null);
    } catch (e) {
      console.warn("Cancel pull failed:", e);
    } finally {
      setDownloadProgress(null);
      await refreshHealth();
    }
  }

  async function handleDeleteModel(tag: string) {
    const p = await core.listProviders();
    const active = p.find((pr) => pr.is_active) ?? p[0];
    if (active) {
      let deleted = false;
      try {
        await core.deleteModel(active.endpoint, tag);
        deleted = true;
      } catch (e) {
        const message = String(e).replace(/^Error:\s*/, "");
        console.error("Delete failed", e);
        window.alert(`Suppression impossible : ${message}`);
      }

      if (!deleted) {
        await refreshHealth();
        return;
      }

      if (health?.active_provider?.model === tag) {
        const remaining = installedModels.filter((m) => m !== tag);
        await core.configureProvider(active.endpoint, remaining[0] ?? null);
      }

      await refreshHealth();
    }
  }

  async function handleDeleteSession(s: Session) {
    if (!window.confirm("Voulez-vous vraiment supprimer cette conversation ?")) return;

    try {
      await core.deleteSession(s.id);
    } catch (e) {
      console.warn("Delete session failed:", e);
    }

    setStandaloneSessions((prev) => prev.filter((item) => item.id !== s.id));
    setSessionsByProject((prev) => ({
      ...prev,
      [s.project_id]: (prev[s.project_id] ?? []).filter((item) => item.id !== s.id),
    }));
    setSessions((prev) => prev.filter((item) => item.id !== s.id));

    if (activeSession?.id === s.id) {
      const remaining = sessions.filter((item) => item.id !== s.id);
      if (remaining.length > 0) {
        setActiveSession(remaining[0]);
      } else if (activeProject) {
        const newS = await core.createSession(activeProject.id);
        setSessionsByProject((prev) => ({ ...prev, [activeProject.id]: [newS] }));
        setSessions([newS]);
        setActiveSession(newS);
      } else {
        setActiveSession(null);
      }
    }
  }

  // Panel Resizing Logic
  function startDrag(panel: string) {
    return (e: React.PointerEvent) => {
      isDragging.current = panel;
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
    };
  }

  function onPointerMove(e: React.PointerEvent) {
    const panel = isDragging.current;
    if (!panel) return;
    // Les panneaux de droite et du bas se mesurent depuis le bord opposé :
    // c'est la distance au bord, pas la position du curseur, qui donne leur
    // taille.
    if (panel === "leftW") setLeftW(clampPanel("leftW", e.clientX));
    else if (panel === "rightW") setRightW(clampPanel("rightW", window.innerWidth - e.clientX));
    else if (panel === "bottomH") setBottomH(clampPanel("bottomH", window.innerHeight - e.clientY));
  }

  /** Déplacer une séparation au clavier. Une poignée qui n'obéit qu'à la souris
   *  rend la taille du panneau inatteignable sans elle. Maj = pas de 40 px.
   *
   *  Le sens suit le geste : pour le panneau de droite, la flèche gauche
   *  pousse la séparation vers la gauche, donc l'agrandit. */
  function nudgePanel(panel: PanelKey) {
    return (e: React.KeyboardEvent) => {
      const step = e.shiftKey ? 40 : 8;
      const [less, more] =
        panel === "bottomH"
          ? (["ArrowDown", "ArrowUp"] as const)
          : panel === "rightW"
            ? (["ArrowRight", "ArrowLeft"] as const)
            : (["ArrowLeft", "ArrowRight"] as const);
      const delta = e.key === less ? -step : e.key === more ? step : 0;
      if (delta === 0) return;
      e.preventDefault();
      const setter = panel === "leftW" ? setLeftW : panel === "rightW" ? setRightW : setBottomH;
      setter((v) => clampPanel(panel, v + delta));
    };
  }

  function onPointerUp(e: React.PointerEvent) {
    if (isDragging.current) {
      (e.target as HTMLElement).releasePointerCapture(e.pointerId);
      isDragging.current = null;
    }
  }

  if (gate === "checking") {
    return (
      <div className="locaryn-app locaryn-connect-wait">
        <span className="locaryn-eveil">Locaryn</span>
      </div>
    );
  }
  if (gate === "connect" && provisioning) {
    return <ConnectScreen provisioning={provisioning} onConnected={() => setGate("ready")} />;
  }

  return (
    <div
      className="locaryn-app locaryn-app-entree"
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      style={{ display: "flex", flexDirection: "column", height: "100vh", overflow: "hidden" }}
    >
      <TopBar
        onToggleRail={() => setLeftOpen((v) => !v)}
        activeView={activeView}
        onSelectView={(v) => setActiveView(v)}
        mode={health?.mode ?? "local"}
        demo={health?.version.includes("demo") ?? false}
        conversationTitle={
          activeSession?.ephemeral ? "Conversation éphémère" : (activeSession?.title ?? null)
        }
        provider={health?.active_provider ?? null}
        showPreview={showPreview}
        showBottom={showBottom}
        showModelConfig={showModelConfig}
        onTogglePreview={() => setShowPreview(!showPreview)}
        onToggleBottom={() => setShowBottom(!showBottom)}
        onToggleModelConfig={() => setShowModelConfig(!showModelConfig)}
        onSettingsClick={() => setPermissionsOpen(true)}
        onChatSettingsClick={() => theme.setSettingsOpen(true)}
        onNewEphemeralChat={
          activeSession?.ephemeral ? handleNewStandaloneChat : handleNewEphemeralChat
        }
        isEphemeral={activeSession?.ephemeral ?? false}
      />

      <ChatPermissionsModal
        isOpen={permissionsOpen}
        onClose={() => setPermissionsOpen(false)}
        trustLevel={activeProject?.trust_level}
        onTrustLevelChange={async (level) => {
          if (!activeProject) return;
          try {
            const updated = await core.updateProject(activeProject.id, undefined, level);
            setActiveProject(updated);
            setProjects((prev) => prev.map((p) => (p.id === updated.id ? updated : p)));
          } catch (e) {
            console.warn("Trust level change failed:", e);
          }
        }}
      />

      {/* Choix du noyau à la création d'une conversation : Locaryn reste le
          défaut ; un noyau installé (OpenClaw, Hermes…) prend la main sur la
          mémoire, les skills et le fournisseur de cette session. */}
      {corePickerFor && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            zIndex: 1200,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            background: "rgba(0,0,0,.45)",
            backdropFilter: "blur(4px)",
          }}
          onMouseDown={(e) => {
            if (e.target === e.currentTarget) setCorePickerFor(null);
          }}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-label="Noyau de la conversation"
            className="locaryn-card"
            style={{ width: 460, maxWidth: "92vw", padding: 20 }}
          >
            <h3 style={{ fontSize: "var(--text-lg)", margin: "0 0 4px" }}>
              Noyau de la conversation
            </h3>
            <p className="locaryn-field-hint" style={{ marginBottom: 16 }}>
              Le noyau décide de la mémoire, des skills et du fournisseur de cette conversation.
              Vous pouvez changer d'avis à chaque nouvelle conversation — le noyau Locaryn n'est
              jamais remplacé.
            </p>
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              <button
                type="button"
                className="locaryn-box-card"
                style={{
                  textAlign: "left",
                  padding: 12,
                  cursor: "pointer",
                  borderColor: "var(--accent)",
                }}
                onClick={() => {
                  setCorePickerFor(null);
                  void createSessionWithCore(corePickerFor, null);
                }}
              >
                <strong style={{ fontSize: 13 }}>Noyau Locaryn (défaut)</strong>
                <span className="locaryn-field-hint" style={{ display: "block" }}>
                  Le noyau natif : vos providers Locaryn, vos règles, vos outils.
                </span>
              </button>
              {installedCores.map((c) => (
                <button
                  key={c.id}
                  type="button"
                  className="locaryn-box-card"
                  style={{ textAlign: "left", padding: 12, cursor: "pointer" }}
                  onClick={() => {
                    setCorePickerFor(null);
                    void createSessionWithCore(corePickerFor, c.id);
                  }}
                >
                  <strong style={{ fontSize: 13 }}>{c.name}</strong>
                  {c.core?.driver && (
                    <span className="locaryn-tag" style={{ marginLeft: 8 }}>
                      {c.core.driver}
                    </span>
                  )}
                  <span className="locaryn-field-hint" style={{ display: "block" }}>
                    {c.description ?? "Noyau alternatif installé."}
                  </span>
                </button>
              ))}
            </div>
            <div style={{ display: "flex", justifyContent: "flex-end", marginTop: 16 }}>
              <button
                type="button"
                className="locaryn-btn-ghost"
                onClick={() => setCorePickerFor(null)}
              >
                Annuler
              </button>
            </div>
          </div>
        </div>
      )}

      <ProjectSettingsModal
        project={projectSettings}
        isOpen={projectSettings !== null}
        onClose={() => setProjectSettings(null)}
        onSave={async (updated) => {
          try {
            const saved = await core.updateProject(updated.id, updated.name, updated.trust_level);
            setProjects((prev) => prev.map((p) => (p.id === saved.id ? saved : p)));
            if (activeProject?.id === saved.id) setActiveProject(saved);
          } catch (e) {
            console.warn("Project save failed:", e);
          } finally {
            setProjectSettings(null);
          }
        }}
      />

      {theme.settingsOpen && (
        <SettingsPanel
          theme={theme}
          onProviderChanged={refreshHealth}
          onOpenFullSettings={() => setActiveView("settings")}
          activeCapabilities={activeCapabilities}
          activeExtensions={activeExtensions}
        />
      )}

      <div
        className="locaryn-body"
        style={{ flex: 1, display: "flex", minHeight: 0, overflow: "hidden" }}
      >
        {leftOpen && (
          <>
            <div style={{ width: leftW, flex: "none" }}>
              <LeftPanel
                projects={projects}
                sessions={sessions}
                sessionsByProject={sessionsByProject}
                standaloneSessions={standaloneSessions}
                activeProject={activeProject}
                activeSession={activeSession}
                onSelectProject={handleSelectProject}
                onSelectSession={handleSelectSession}
                onNewSession={handleNewSession}
                onNewStandaloneChat={handleNewStandaloneChat}
                onAddProject={handleAddProject}
                onDeleteSession={handleDeleteSession}
                onSessionArchived={handleArchiveSession}
                onSessionMoved={handleMoveSession}
                onSessionRenamed={handleRenameSession}
                onSessionsMerged={handleMergeSessions}
                onNewEphemeralChat={handleNewEphemeralChat}
                activeView={activeView}
                onSelectView={(v) => setActiveView(v as typeof activeView)}
                activeCapabilities={activeCapabilities}
                installedExtensions={activeExtensions}
                onOpenProjectSettings={(p) => setProjectSettings(p)}
                onProjectArchived={(p) => {
                  // Drop it from the sidebar and fall back to another project.
                  setProjects((prev) => {
                    const next = prev.filter((x) => x.id !== p.id);
                    if (activeProject?.id === p.id) {
                      const fallback = next[0] ?? null;
                      setActiveProject(fallback);
                      setSessions([]);
                      setActiveSession(null);
                      if (fallback) handleSelectProject(fallback);
                    }
                    return next;
                  });
                  setSessionsByProject((prev) => {
                    const next = { ...prev };
                    delete next[p.id];
                    return next;
                  });
                }}
              />
            </div>
            <div
              className="locaryn-resizer locaryn-resizer-v"
              onPointerDown={startDrag("leftW")}
              onKeyDown={nudgePanel("leftW")}
              role="separator"
              aria-orientation="vertical"
              aria-label="Largeur du panneau latéral"
              aria-valuenow={leftW}
              aria-valuemin={160}
              aria-valuemax={500}
              tabIndex={0}
            />
          </>
        )}

        {/* Le chat est la couche de fond : il reste monte quel que soit
            l'ecran ouvert par-dessus. Le demonter et le remonter a chaque
            aller-retour vers les reglages perdait la position de defilement et
            relancait le rendu de toute la conversation. */}
        <ChatPanel
          sessionId={activeSession?.id ?? null}
          projectId={activeProject?.id ?? null}
          connectionMode={health?.mode}
          coreName={
            activeSession?.core_id
              ? (installedCores.find((c) => c.id === activeSession.core_id)?.name ??
                activeSession.core_id)
              : null
          }
          onCreateSessionForPrompt={handleCreateSessionForPrompt}
          onSessionMoved={(projectId) => {
            if (activeSession) void handleMoveSession(activeSession, projectId);
          }}
          onOpenSettings={() => setActiveView("settings")}
          ephemeral={activeSession?.ephemeral ?? false}
          onNewChat={handleNewStandaloneChat}
          onAddProject={async () => {
            const path = await pickFolder();
            if (!path) return;
            const name =
              window.prompt(
                "Nom du projet:",
                path
                  .replace(/[\\/]+$/, "")
                  .split(/[\\/]/)
                  .pop() ?? "projet",
              ) ?? "projet";
            handleAddProject(path, name);
          }}
          onOpenMarketplace={() => setActiveView("models")}
          activeCapabilities={activeCapabilities}
          extensions={activeExtensions}
        />

        {/* La fenetre de mise a jour vit au-dessus de tout, y compris de la
          surcouche : elle ne s'affiche que lorsqu'il y a quelque chose a dire,
          et ce qu'elle dit vaut pour l'application entiere. */}
        <UpdateDialog />

        {/* Les ecrans autres que le chat s'ouvrent par-dessus lui, pas a sa
            place.
            Les faire cohabiter dans le meme espace obligeait le rail de gauche
            a changer de contenu selon l'ecran — et donc de tete. En surcouche,
            le chat et son historique restent montes dessous, le rail ne bouge
            jamais, et l'ecran ouvert dispose de toute la fenetre.
            Porte dans `document.body` : `.locaryn-app` porte un transform
            d'entree, qui ferait d'elle le bloc conteneur de tout
            `position: fixed` descendant. */}
        {activeView !== "chat" &&
          createPortal(
            <div className="locaryn-overlay">
              <button
                type="button"
                className="locaryn-overlay-scrim"
                aria-label="Fermer"
                onClick={() => setActiveView("chat")}
              />
              <div
                className="locaryn-overlay-panel"
                role="dialog"
                aria-modal="true"
                aria-label={OVERLAY_LABELS[activeView] ?? "Panneau"}
              >
                <button
                  type="button"
                  className="locaryn-overlay-close"
                  title="Fermer (Échap)"
                  onClick={() => setActiveView("chat")}
                >
                  <Icon name="close" size={18} />
                </button>
                {activeView === "models" && (
                  <div className="locaryn-view-container">
                    <div className="locaryn-view-header">
                      {/* Un titre est un nom, pas une parenthèse d'explication : la
                  provenance et la liste des familles vivent dans le contenu,
                  qui les montre déjà. */}
                      <h2>Marketplace</h2>
                      <p className="locaryn-view-desc">
                        Le catalogue complet. Chaque famille regroupe ses tailles, et chaque taille
                        ses quantifications.
                      </p>
                    </div>
                    <ModelBrowser
                      onInstall={handleInstallModel}
                      onCancelInstall={handleCancelDownload}
                      onDelete={handleDeleteModel}
                      installed={installedModels}
                      activeCapabilities={activeCapabilities}
                      activeExtensions={activeExtensions}
                      onOpenTraining={() => setActiveView("training")}
                      onSelectModelForChat={async (tag) => {
                        try {
                          const providers = await core.listProviders();
                          const active = providers.find((p) => p.is_active) ?? providers[0];
                          if (active) await core.configureProvider(active.endpoint, tag);
                        } catch (e) {
                          console.warn("configureProvider failed, navigating anyway:", e);
                        }
                        setActiveView("chat");
                        refreshHealth();
                      }}
                      onLaunchAirllm={async (repo) => {
                        try {
                          await core.configureAirllmProvider(repo);
                        } catch (e) {
                          console.warn("configureAirllmProvider failed, navigating anyway:", e);
                        }
                        setActiveView("chat");
                        refreshHealth();
                      }}
                    />
                  </div>
                )}

                {activeView === "installed" && (
                  <InstalledModelsView
                    installedModels={installedModels}
                    onSelectModelForChat={async (modelTag) => {
                      try {
                        const providers = await core.listProviders();
                        const active = providers.find((p) => p.is_active) ?? providers[0];
                        if (active) await core.configureProvider(active.endpoint, modelTag);
                      } catch (e) {
                        console.warn("configureProvider failed:", e);
                      }
                      setActiveView("chat");
                      refreshHealth();
                    }}
                    onDeleteModel={handleDeleteModel}
                    onOpenMarketplace={() => setActiveView("models")}
                    extensions={activeExtensions}
                  />
                )}

                {activeView === "batch" && <BatchStudio />}

                {activeView === "figures" && (
                  <FiguresView
                    onOpenSession={(sess) => {
                      handleSelectSession(sess);
                      setActiveView("chat");
                    }}
                    onNewWithFigure={async (f) => {
                      // Une conversation neuve, confiée à la figure : ses consignes
                      // partent avec le premier message, sans que personne ait à les
                      // recopier.
                      try {
                        const project = freeProject ?? (await core.freeChatProject());
                        if (!freeProject) setFreeProject(project);
                        const sess = await core.createSession(project.id);
                        await core.attachFigure(sess.id, f.id);
                        setStandaloneSessions((prev) => [sess, ...prev]);
                        setActiveProject(null);
                        setActiveSession(sess);
                        setActiveView("chat");
                      } catch (e) {
                        console.error(e);
                      }
                    }}
                  />
                )}

                {activeView === "studio" && (
                  <StudioView
                    installedModels={installedModels}
                    extensions={activeExtensions}
                    onCloseAudioGen={() => setActiveView("chat")}
                  />
                )}

                {/* Plus dans le menu : on y arrive depuis le catalogue de modèles,
            sur lequel ce studio agit. */}
                {activeView === "training" && (
                  <ModelStudioView
                    onOpenMarketplace={() => setActiveView("models")}
                    onOpenSettings={() => setActiveView("settings")}
                  />
                )}

                {/* Un écran déclaré par une extension : l'application ne connaît pas
            son nom, elle sait seulement qu'une extension le revendique. */}
                <ExtensionScreen view={activeView} extensions={activeExtensions} />

                {activeView === "extensions" && (
                  <div className="locaryn-view-container">
                    <ExtensionsSettings />
                  </div>
                )}

                {activeView === "connectors" && (
                  <div className="locaryn-view-container">
                    <ConnectorsSettings />
                  </div>
                )}

                {activeView === "account" && (
                  <SettingsView
                    theme={theme}
                    projects={projects}
                    sessionsByProject={sessionsByProject}
                    standaloneSessions={standaloneSessions}
                    activeCapabilities={activeCapabilities}
                    initialSection="account"
                    onOpenSession={(session) => {
                      void handleSelectSession(session);
                      setActiveView("chat");
                    }}
                    onOpenMarketplace={() => setActiveView("models")}
                    onProjectArchived={(p) => {
                      setProjects((prev) => prev.filter((x) => x.id !== p.id));
                      setSessionsByProject((prev) => {
                        const next = { ...prev };
                        delete next[p.id];
                        return next;
                      });
                    }}
                  />
                )}

                {activeView === "settings" && (
                  <SettingsView
                    theme={theme}
                    projects={projects}
                    sessionsByProject={sessionsByProject}
                    standaloneSessions={standaloneSessions}
                    activeCapabilities={activeCapabilities}
                    initialSection={settingsInitialSection}
                    onOpenSession={(session) => {
                      void handleSelectSession(session);
                      setActiveView("chat");
                    }}
                    onOpenMarketplace={() => setActiveView("models")}
                    onProjectArchived={(p) => {
                      setProjects((prev) => prev.filter((x) => x.id !== p.id));
                      setSessionsByProject((prev) => {
                        const next = { ...prev };
                        delete next[p.id];
                        return next;
                      });
                      if (activeProject?.id === p.id) {
                        setActiveProject(null);
                        setSessions([]);
                        setActiveSession(null);
                      }
                    }}
                  />
                )}
              </div>
            </div>,
            document.body,
          )}

        {/* Right side panels for Chat view */}
        {showModelConfig && (
          <>
            <div
              className="locaryn-resizer locaryn-resizer-v"
              onPointerDown={startDrag("rightW")}
              onKeyDown={nudgePanel("rightW")}
              role="separator"
              aria-orientation="vertical"
              aria-label="Largeur du panneau de droite"
              aria-valuenow={rightW}
              aria-valuemin={PANEL_LIMITS.rightW.min}
              aria-valuemax={PANEL_LIMITS.rightW.max}
              tabIndex={0}
            />
            <div style={{ width: rightW, flex: "none", display: "flex", minWidth: 0 }}>
              <ModelConfigPanel onClose={() => setShowModelConfig(false)} />
            </div>
          </>
        )}

        {showPreview && (
          <>
            <div
              className="locaryn-resizer locaryn-resizer-v"
              onPointerDown={startDrag("rightW")}
              onKeyDown={nudgePanel("rightW")}
              role="separator"
              aria-orientation="vertical"
              aria-label="Largeur du panneau d'aperçu"
              aria-valuenow={rightW}
              aria-valuemin={PANEL_LIMITS.rightW.min}
              aria-valuemax={PANEL_LIMITS.rightW.max}
              tabIndex={0}
            />
            <div style={{ width: rightW, flex: "none", display: "flex", minWidth: 0 }}>
              <RunPanel />
            </div>
          </>
        )}
      </div>

      {activeView === "chat" && showBottom && (
        <>
          <div
            className="locaryn-resizer locaryn-resizer-h"
            onPointerDown={startDrag("bottomH")}
            onKeyDown={nudgePanel("bottomH")}
            role="separator"
            aria-orientation="horizontal"
            aria-label="Hauteur du panneau du bas"
            aria-valuenow={bottomH}
            aria-valuemin={PANEL_LIMITS.bottomH.min}
            aria-valuemax={PANEL_LIMITS.bottomH.max}
            tabIndex={0}
          />
          <div
            style={{ height: bottomH, width: "100%", flex: "none", display: "flex", minHeight: 0 }}
          >
            <BottomPanel cwd={activeProject?.path ?? null} sessionId={activeSession?.id ?? null} />
          </div>
        </>
      )}

      {/* Barre d'état : à gauche l'état du service et le modèle chargé, au
          centre ce qui tourne, à droite le centre de notifications. */}
      <footer className="locaryn-footer-bar">
        <div className="locaryn-footer-left">
          <ModelResidency />
        </div>
        <div className="locaryn-footer-center">
          <RunningTask />
        </div>
        <div className="locaryn-footer-actions">
          {downloadProgress && (
            <>
              <span
                className="locaryn-footer-text"
                title={downloadProgress.status ?? `Téléchargement de ${downloadProgress.tag}`}
              >
                {downloadProgress.status
                  ? `${downloadProgress.status} (${downloadProgress.progress} %)`
                  : `Téléchargement de ${downloadProgress.tag} — ${downloadProgress.progress} %`}
              </span>
              <div style={{ width: "120px" }}>
                <LoProgress
                  value={downloadProgress.progress / 100}
                  on="surface"
                  label={`Téléchargement de ${downloadProgress.tag}`}
                />
              </div>
              <button
                type="button"
                className="locaryn-btn-ghost"
                style={{
                  color: "var(--danger)",
                  border: "1px solid var(--danger)",
                  padding: "2px 8px",
                  fontSize: "11px",
                }}
                onClick={handleCancelDownload}
                title="Annuler le téléchargement en cours"
              >
                <Icon name="close" size={15} /> Annuler
              </button>
            </>
          )}
          {/* Notification center — always visible (downloads, generations, workflows). */}
          <TaskCenter />
        </div>
      </footer>
    </div>
  );
}
