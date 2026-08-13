import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { BatchStudio } from "./components/BatchStudio";
import { ChatPermissionsModal } from "./components/ChatPermissionsModal";
import { ConnectScreen } from "./components/ConnectScreen";
import { ConnectorsSettings } from "./components/ConnectorsSettings";
import { ModelBrowser } from "./components/ModelBrowser";
import { ModelResidency } from "./components/ModelResidency";
import { NavDrawer } from "./components/NavDrawer";
import { ProjectSettingsModal } from "./components/ProjectSettingsModal";
import { SettingsPanel } from "./components/SettingsPanel";
import { TaskCenter } from "./components/TaskCenter";
import { TopBar } from "./components/TopBar";
import { useTheme } from "./hooks/useTheme";
import { FREE_CHAT_PATH } from "./lib/constants";
import {
  type Health,
  type Project,
  type Provisioning,
  type Session,
  core,
  coreMode,
} from "./lib/core";
import { parseInstallLink, setPendingInstall } from "./lib/deepLink";
import { pickFolder } from "./lib/dialog";
import { setRunReveal } from "./lib/runPanel";
import { taskCenter } from "./lib/taskCenter";
import { BottomPanel } from "./panels/BottomPanel";
import { ChatPanel } from "./panels/ChatPanel";
import { LeftPanel } from "./panels/LeftPanel";
import { ModelConfigPanel } from "./panels/ModelConfigPanel";
import { RunPanel } from "./panels/RunPanel";
import { InstalledModelsView } from "./views/InstalledModelsView";
import { ModelStudioView } from "./views/ModelStudioView";
import { SettingsView } from "./views/SettingsView";
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

export function App() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [activeProject, setActiveProject] = useState<Project | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [standaloneSessions, setStandaloneSessions] = useState<Session[]>([]);
  /** Hidden project that owns free (project-less) chats. */
  const [freeProject, setFreeProject] = useState<Project | null>(null);
  const [activeSession, setActiveSession] = useState<Session | null>(null);
  const [health, setHealth] = useState<Health | null>(null);
  const [installedModels, setInstalledModels] = useState<string[]>([]);

  const theme = useTheme();

  const [downloadProgress, setDownloadProgress] = useState<{
    tag: string;
    progress: number;
    status?: string;
  } | null>(null);

  // Active top-level view: "chat" | "models" | "studio" | "training" | "connectors" | "settings" | "account"
  const [activeView, setActiveView] = useState<string>("chat");

  // Toggleable panels & drawers
  const [navDrawerOpen, setNavDrawerOpen] = useState(false);
  // Popup « installer un modèle spécifique » (bouton + du marketplace).
  const [customInstallOpen, setCustomInstallOpen] = useState(false);
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
  const [showImageGen, setShowImageGen] = useState(false);
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
      if (realProjects.length > 0 && !activeProject) {
        setActiveProject(realProjects[0]);
      }

      if (free) {
        setStandaloneSessions(await core.listSessions(free.id).catch(() => []));
      }

      if (b.project && b.project.id !== free?.id) {
        const sList = await core.listSessions(b.project.id);
        setSessions(sList);
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
      if (active) {
        const models = await core.listModels(active.endpoint);
        setInstalledModels(models);
      }
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
    if (!proj) return;
    try {
      const sList = await core.listSessions(proj.id);
      setSessions(sList);
      if (sList.length > 0) {
        setActiveSession(sList[0]);
      } else {
        const newS = await core.createSession(proj.id);
        setSessions([newS]);
        setActiveSession(newS);
      }
    } catch (e) {
      console.warn("Select project failed:", e);
    }
  }

  async function handleSelectSession(session: Session) {
    setActiveSession(session);
  }

  async function handleNewSession(proj: Project) {
    try {
      const newS = await core.createSession(proj.id);
      setSessions((prev) => [newS, ...prev]);
      setActiveSession(newS);
      setActiveView("chat");
    } catch (e) {
      console.warn("Create session failed:", e);
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
  async function handleCreateSessionForPrompt(firstPrompt: string): Promise<Session | null> {
    try {
      // Chats started from the home screen belong to the active project when
      // there is one, otherwise to the hidden free-chat project.
      let project = activeProject;
      if (!project) {
        project = freeProject ?? (await core.freeChatProject());
        if (!freeProject) setFreeProject(project);
      }
      const s = await core.createSession(project.id);
      if (!activeProject) {
        setStandaloneSessions((prev) => [s, ...prev]);
      } else {
        setSessions((prev) => [s, ...prev]);
      }
      setActiveSession(s);

      // Ask the LLM for a concise title in the background.
      core
        .generateSessionTitle(s.id, firstPrompt)
        .then((title) => {
          const newS = { ...s, title };
          if (!activeProject) {
            setStandaloneSessions((prev) => prev.map((x) => (x.id === s.id ? newS : x)));
          } else {
            setSessions((prev) => prev.map((x) => (x.id === s.id ? newS : x)));
          }
          if (activeSession?.id === s.id) {
            setActiveSession(newS);
          }
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
      // `heretic` makes the backend auto-install the uncensored companions.
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
      );
      setDownloadProgress({ tag, progress: 100, status: "Téléchargement terminé ✓" });
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
      try {
        await core.deleteModel(active.endpoint, tag);
      } catch (e) {
        console.error("Delete failed", e);
      }

      if (health?.active_provider?.model === tag) {
        const remaining = installedModels.filter((m) => m !== tag);
        await core.configureProvider(active.endpoint, remaining[0] ?? "gemma2:2b");
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
    setSessions((prev) => prev.filter((item) => item.id !== s.id));

    if (activeSession?.id === s.id) {
      const remaining = sessions.filter((item) => item.id !== s.id);
      if (remaining.length > 0) {
        setActiveSession(remaining[0]);
      } else if (activeProject) {
        const newS = await core.createSession(activeProject.id);
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
    return <div className="locaryn-app locaryn-connect-wait" />;
  }
  if (gate === "connect" && provisioning) {
    return <ConnectScreen provisioning={provisioning} onConnected={() => setGate("ready")} />;
  }

  return (
    <div
      className="locaryn-app"
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      style={{ display: "flex", flexDirection: "column", height: "100vh", overflow: "hidden" }}
    >
      <TopBar
        activeView={activeView}
        onSelectView={(v) => setActiveView(v)}
        mode={health?.mode ?? "local"}
        demo={health?.version.includes("demo") ?? false}
        project={activeProject?.name ?? "syncho"}
        provider={health?.active_provider ?? null}
        showPreview={showPreview}
        showBottom={showBottom}
        showModelConfig={showModelConfig}
        onToggleNavDrawer={() => setNavDrawerOpen(true)}
        onOpenCustomInstall={() => setCustomInstallOpen(true)}
        onTogglePreview={() => setShowPreview(!showPreview)}
        onToggleBottom={() => setShowBottom(!showBottom)}
        onToggleModelConfig={() => setShowModelConfig(!showModelConfig)}
        onSettingsClick={() => setPermissionsOpen(true)}
        onChatSettingsClick={() => theme.setSettingsOpen(true)}
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
        />
      )}

      <NavDrawer
        isOpen={navDrawerOpen}
        onClose={() => setNavDrawerOpen(false)}
        activeView={activeView}
        onSelectView={(v) => setActiveView(v)}
      />

      <div
        className="locaryn-body"
        style={{ flex: 1, display: "flex", minHeight: 0, overflow: "hidden" }}
      >
        {activeView === "chat" && leftOpen && (
          <>
            <div style={{ width: leftW, flex: "none" }}>
              <LeftPanel
                projects={projects}
                sessions={sessions}
                standaloneSessions={standaloneSessions}
                activeProject={activeProject}
                activeSession={activeSession}
                onSelectProject={handleSelectProject}
                onSelectSession={handleSelectSession}
                onNewSession={handleNewSession}
                onNewStandaloneChat={handleNewStandaloneChat}
                onAddProject={handleAddProject}
                onDeleteSession={handleDeleteSession}
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

        {activeView === "chat" && (
          <ChatPanel
            sessionId={activeSession?.id ?? null}
            projectId={activeProject?.id ?? null}
            connectionMode={health?.mode}
            onCreateSessionForPrompt={handleCreateSessionForPrompt}
            onOpenSettings={() => setActiveView("settings")}
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
            onAddSsh={() => setActiveView("connectors")}
            onOpenMarketplace={() => setActiveView("models")}
            forceOpenImageGen={showImageGen}
            onImageGenClosed={() => setShowImageGen(false)}
          />
        )}

        {activeView === "models" && (
          <div className="locaryn-view-container">
            <div className="locaryn-view-header">
              <h2>Marketplace Modèles (HuggingFace Hub & Modèles Locaux)</h2>
              <p className="locaryn-view-desc">
                Explorez, installez et gérez vos modèles d'IA locaux (Gemma 2 E2B/E4B, Instruct 💬,
                Audio 🎙️, Kimi K3, MiMo, GLM 5.2...).
              </p>
            </div>
            <ModelBrowser
              customInstallOpen={customInstallOpen}
              onCloseCustomInstall={() => setCustomInstallOpen(false)}
              onInstall={handleInstallModel}
              onCancelInstall={handleCancelDownload}
              onDelete={handleDeleteModel}
              installed={installedModels}
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
              onOpenImageGen={() => {
                setActiveView("chat");
                // Le ChatPanel ouvrira le panneau image via son état interne
                // On passe par le state global imageGenOpen
                setTimeout(() => setShowImageGen(true), 50);
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
            onOpenImageGen={() => {
              setActiveView("chat");
              setTimeout(() => setShowImageGen(true), 50);
            }}
            onDeleteModel={handleDeleteModel}
            onOpenMarketplace={() => setActiveView("models")}
          />
        )}

        {activeView === "batch" && <BatchStudio />}

        {activeView === "studio" && (
          <StudioView
            installedModels={installedModels}
            installedImageModels={installedModels}
            onOpenImageGen={() => {
              setActiveView("chat");
              setShowImageGen(true);
            }}
            onCloseAudioGen={() => setActiveView("chat")}
            onSendImageToChat={async (url, label) => {
              // Append the image to the active chat session, then switch to chat view.
              if (activeSession) {
                try {
                  await core.appendAssistantMessage(activeSession.id, `🖼️ ${label}\n\n![](${url})`);
                } catch (e) {
                  console.warn("Failed to append image message:", e);
                }
              }
              setActiveView("chat");
            }}
          />
        )}

        {activeView === "training" && (
          <ModelStudioView
            onOpenMarketplace={() => setActiveView("models")}
            onOpenSettings={() => setActiveView("settings")}
          />
        )}

        {activeView === "connectors" && (
          <div className="locaryn-view-container">
            <ConnectorsSettings />
          </div>
        )}

        {activeView === "settings" && (
          <SettingsView
            theme={theme}
            projects={projects}
            onOpenMarketplace={() => setActiveView("models")}
            onProjectArchived={(p) => {
              setProjects((prev) => prev.filter((x) => x.id !== p.id));
              if (activeProject?.id === p.id) {
                setActiveProject(null);
                setSessions([]);
                setActiveSession(null);
              }
            }}
          />
        )}

        {/* Right side panels for Chat view */}
        {activeView === "chat" && showModelConfig && (
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

        {activeView === "chat" && showPreview && (
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
          <div style={{ height: bottomH, flex: "none", display: "flex", minHeight: 0 }}>
            <BottomPanel cwd={activeProject?.path ?? null} sessionId={activeSession?.id ?? null} />
          </div>
        </>
      )}

      {/* Global Live Footer Status & Download Progress Bar */}
      <footer className="locaryn-footer-bar">
        {/* Moitié gauche : le modèle en mémoire et la main dessus. Les
            téléchargements sont passés à droite, avec leur barre et leur
            bouton d'annulation — c'est une notification, pas un état. */}
        <div className="locaryn-footer-left">
          <ModelResidency />
        </div>
        <div
          style={{
            display: "flex",
            gap: "10px",
            fontSize: "11px",
            color: "var(--text-faint)",
            alignItems: "center",
          }}
        >
          {downloadProgress && (
            <>
              <span className="locaryn-footer-text">
                {downloadProgress.status
                  ? `${downloadProgress.status} (${downloadProgress.progress} %)`
                  : `Téléchargement de ${downloadProgress.tag} — ${downloadProgress.progress} %`}
              </span>
              <div className="locaryn-footer-progress-track" style={{ width: "120px" }}>
                <div
                  className="locaryn-footer-progress-fill"
                  style={{ width: `${downloadProgress.progress}%` }}
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
                ⛔ Annuler
              </button>
            </>
          )}
          {/* Notification center — always visible (downloads, generations, workflows). */}
          <TaskCenter
            onReopenImageGen={() => {
              setActiveView("chat");
              setTimeout(() => setShowImageGen(true), 50);
            }}
          />
        </div>
      </footer>
    </div>
  );
}
