import { Icon, isIconName } from "@locaryn/ui-core";
import { useCallback, useEffect, useRef, useState } from "react";
import { ImageGenPanel } from "../components/ImageGenPanel";
import { QuickModelSelector } from "../components/QuickModelSelector";
import { RagPanel } from "../components/RagPanel";
import { ToolApprovalModal } from "../components/ToolApprovalModal";
import { ImageIntentCard } from "../components/chat/ImageIntentCard";
import { MessageBubble } from "../components/chat/MessageBubble";
import { ProjectSuggestion } from "../components/chat/ProjectSuggestion";
import { ReasoningPicker } from "../components/chat/ReasoningPicker";
import { ToolCard } from "../components/chat/ToolCard";
import { VoiceNote } from "../components/chat/VoiceNote";
import { WorkspacePicker, type WorkspaceSelection } from "../components/chat/WorkspacePicker";
import { ExtensionSlot } from "../components/extensions/ExtensionSlot";
import { FREE_CHAT_PATH } from "../lib/constants";
import {
  type ConnectionMode,
  type ExtensionComposerAction,
  IMAGE_QUALITIES,
  type ImageIntent,
  type ReasoningLevel,
  type Session,
  type StreamEvent,
  type ToolApprovalDecision,
  type ToolApprovalRequest,
  core,
  reasoningPayload,
} from "../lib/core";
import { pickSaveFile } from "../lib/dialog";
import { recordTextGenerationDuration } from "../lib/durationEstimator";
import { setImageResultHandler, startImageGeneration, toImageUrl } from "../lib/imageJobs";
import { loadMediaObjectUrl } from "../lib/media";
import { pluginBridge } from "../lib/pluginBridge";
import { appendRunLine, finishTerminalRun, showWebRun, startTerminalRun } from "../lib/runPanel";
import {
  type SlashAction,
  type SlashCommand,
  type SlashSuggestion,
  argToSize,
  matchExtensionCommand,
  matchSlashInput,
} from "../lib/slashCommands";
import { runWorkflow } from "../lib/workflow";
import { DEFAULT_MODEL_PARAMS } from "./ModelConfigPanel";

type ChatItem =
  | {
      id: string;
      kind: "msg";
      role: "user" | "assistant";
      text: string;
      images?: string[];
      imagePaths?: Array<string | undefined>;
    }
  | {
      id: string;
      kind: "audio";
      path: string;
      url: string | null;
      status: "loading" | "ready" | "error";
      error?: string;
    }
  | {
      id: string;
      kind: "tool";
      callId: string;
      tool: string;
      args: unknown;
      status: "running" | "ok" | "error";
      output: string;
    }
  | { id: string; kind: "log"; text: string }
  | {
      id: string;
      kind: "intent";
      intent: ImageIntent;
      model: string;
      /** Original message, replayed as a normal turn when refused. */
      original: string;
      decided?: "accepted" | "refused";
    };

type Attachment = { id: string; dataUrl: string; base64: string; name: string };
type QueuedMessage = { id: string; text: string; attachments: Attachment[] };

/** Identités locales pour les clés React. Rien de ce qui s'affiche dans le fil
 *  n'a d'identifiant propre avant d'être écrit en base — jetons diffusés,
 *  journaux, cartes d'intention — et une entrée peut être retirée du milieu
 *  (reprise du dernier message), donc l'index ne tient pas. */
let idSeq = 0;
function nextId(prefix: string): string {
  idSeq += 1;
  return `${prefix}-${idSeq}`;
}

type Props = {
  sessionId: string | null;
  projectId?: string | null;
  /** Rien de cette conversation ne sera gardé : l'écran doit le dire. */
  ephemeral?: boolean;
  ctxSize?: number;
  onOpenMarketplace?: () => void;
  forceOpenImageGen?: boolean;
  onImageGenClosed?: () => void;
  /** Home screen: create (and auto-name) a chat from the very first prompt. */
  onCreateSessionForPrompt?: (
    firstPrompt: string,
    targetProjectId?: string | null,
  ) => Promise<Session | null>;
  /** Slash commands: open the application settings. */
  onOpenSettings?: () => void;
  /** Slash commands: start a fresh conversation. */
  onNewChat?: () => void;
  /** Workspace picker: add a local folder. */
  onAddProject?: () => void;
  /** Workspace picker: register a new SSH connection. */
  onAddSsh?: () => void;
  /** Connection mode from the Rust core. Cloud workspace is only enabled
   *  when the app is connected to a remote Locaryn server (mode === "remote"). */
  connectionMode?: ConnectionMode;
  /** La conversation vient d'être rangée dans un projet, sur proposition du
   *  petit modèle : les listes doivent suivre. */
  onSessionMoved?: (projectId: string) => void;
  /** Nom du noyau alternatif qui pilote cette conversation (OpenClaw,
   *  Hermes…). Absent = noyau Locaryn natif. */
  coreName?: string | null;
  activeCapabilities?: string[];
};

const SUGGESTIONS = [
  "Explique la structure de ce projet",
  "Écris des tests unitaires pour la logique principale",
  "Cherche les commentaires FIXME et TODO",
];

/** In "auto" mode, force JSON only when the prompt actually asks for it. */
function wantsJson(prompt: string): boolean {
  return /\b(json|structur\w+|sch[ée]ma|format\w*\s+json|cl[ée]s?\/valeurs?)\b/i.test(prompt);
}

/** Cheap local guess: is this message plausibly asking for an image?
 *  Only then do we pay for a model round-trip to confirm and translate it. */
function looksLikeImageRequest(text: string): boolean {
  return /(image|photo|dessin|dessine|logo|ic[oô]ne|illustration|visuel|rendu|affiche|picture|draw|render)/i.test(
    text,
  );
}

/** Pull inline `![](…)` images out of a stored message so they render as
 *  real images (the markdown renderer has no image support). Handles data:
 *  URLs, Tauri asset:// URLs, and any other image scheme. */
function splitInlineImages(content: string): { text: string; images?: string[] } {
  const images: string[] = [];
  const text = content
    .replace(/!\[[^\]]*\]\(([^)]+)\)/g, (_m, url: string) => {
      images.push(url);
      return "";
    })
    .trim();
  return images.length ? { text, images } : { text: content };
}

/** Audio artifacts are persisted as transparent HTML comments so a session
 *  can restore the playable note without showing a filesystem path to the
 *  model or the user. */
function splitInlineAudio(content: string): { text: string; paths: string[] } {
  const paths: string[] = [];
  const text = content
    .replace(/<!--locaryn-audio:([\s\S]*?)-->/g, (_m, encoded: string) => {
      try {
        const path = JSON.parse(encoded);
        if (typeof path === "string" && path) paths.push(path);
      } catch {
        // Ignore a malformed marker rather than exposing implementation data.
      }
      return "";
    })
    .trim();
  return { text, paths };
}

function storedMessageItems(m: { id: string; role: string; content: string }): ChatItem[] {
  const audio = splitInlineAudio(m.content);
  const parsed = splitInlineImages(audio.text);
  const result: ChatItem[] = [];
  if (parsed.text || parsed.images?.length) {
    result.push({
      id: m.id,
      kind: "msg",
      role: m.role as "user" | "assistant",
      text: parsed.text,
      images: parsed.images,
    });
  }
  audio.paths.forEach((path, index) => {
    result.push({
      id: `${m.id}:audio:${index}`,
      kind: "audio",
      path,
      url: null,
      status: "loading",
    });
  });
  return result;
}

function readFile(file: File): Promise<Attachment> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const dataUrl = reader.result as string;
      const base64 = dataUrl.replace(/^data:[^;]+;base64,/, "");
      resolve({ id: nextId("att"), dataUrl, base64, name: file.name });
    };
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
}

export function ChatPanel({
  sessionId,
  projectId,
  ephemeral,
  ctxSize,
  onOpenMarketplace,
  forceOpenImageGen,
  onImageGenClosed,
  onCreateSessionForPrompt,
  onOpenSettings,
  onNewChat,
  onAddProject,
  onAddSsh,
  connectionMode,
  onSessionMoved,
  coreName,
  activeCapabilities = [],
}: Props) {
  const [items, setItems] = useState<ChatItem[]>([]);
  const [input, setInput] = useState("");
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  /** JSON output: "auto" (detect from the prompt), "on" or "off".
   *  No composer chip — it wasted space; `/json` cycles it when needed. */
  const [jsonMode, setJsonMode] = useState<"auto" | "on" | "off">("auto");
  const [reasoning, setReasoning] = useState<ReasoningLevel>("auto");
  const [messageQueue, setMessageQueue] = useState<QueuedMessage[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [activeModel, setActiveModel] = useState<string>("");
  const [installedModels, setInstalledModels] = useState<string[]>([]);
  const [quickModelOpen, setQuickModelOpen] = useState(false);
  const [imageGenOpen, setImageGenOpen] = useState(false);
  const [ragOpen, setRagOpen] = useState(false);
  /** Indexed chunk count — shown on the Documents chip so the feature is discoverable. */
  const [ragCount, setRagCount] = useState(0);
  const [isLocalModel, setIsLocalModel] = useState(true);
  /** First installed diffusion checkpoint — used when a chat message is
   *  routed to image generation. */
  const [activeImageModel, setActiveImageModel] = useState("");
  /** Where the agent works: a local folder, an SSH server, a remote environment, or standalone. */
  const [workspace, setWorkspace] = useState<WorkspaceSelection>({
    kind: "none",
    id: null,
    label: "Dossier de travail",
  });
  /** Directory the code runner and terminal should use for this session. */
  const [runCwd, setRunCwd] = useState<string | null>(null);
  /** One-click next-step suggestions, asked to the model after each answer. */
  const [followups, setFollowups] = useState<string[]>([]);
  const [followupsLoading, setFollowupsLoading] = useState(false);
  /** Slash-command palette: matches for the current input, and the highlighted row. */
  const [slash, setSlash] = useState<SlashSuggestion | null>(null);
  /** Demande d'approbation en cours. `null` = aucune fenêtre ouverte. */
  const [approval, setApproval] = useState<ToolApprovalRequest | null>(null);
  // Commandes apportees par les plugins actifs. Rechargees au montage :
  // installer ou desactiver une extension change la palette sans redemarrage.
  const [extCommands, setExtCommands] = useState<SlashCommand[]>([]);
  const [composerError, setComposerError] = useState<string | null>(null);
  const [slashIndex, setSlashIndex] = useState(0);
  /** Resolution requested through a slash argument (e.g. "/image max"). */
  const [slashSize, setSlashSize] = useState<number | null>(null);
  /** Set by /plan: the next message is executed as a step-by-step plan. */
  const [planNext, setPlanNext] = useState(false);

  const streamRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const queueRef = useRef<QueuedMessage[]>([]);
  queueRef.current = messageQueue;
  /** Session we just created from the home screen: skip the DB reload once so
   *  the optimistic first message isn't wiped by an empty fetch. */
  const skipLoadRef = useRef<string | null>(null);
  /** Text of the answer being streamed — used to ask for follow-up suggestions. */
  const fullTextRef = useRef<string>("");

  // Mémorisée sans dépendance : elle ne referme que des setters d'état, donc
  // l'effet de montage peut la citer sans se relancer à chaque rendu.
  const refreshActiveModel = useCallback(async () => {
    try {
      const providers = await core.listProviders();
      const active = providers.find((p) => p.is_active) ?? providers[0];
      const endpoint = active?.endpoint ?? "http://127.0.0.1:8080";

      let list: string[] = [];
      try {
        list = await core.listModels(endpoint);
        setInstalledModels(list);
      } catch {
        list = active?.model ? [active.model] : [];
        setInstalledModels(list);
      }

      let targetModel = active?.model ?? "";
      // Si le modèle enregistré n'existe pas dans les modèles installés, on sélectionne le premier installé
      if ((!targetModel || (list.length > 0 && !list.includes(targetModel))) && list.length > 0) {
        targetModel = list[0];
        if (active) {
          void core.configureProvider(active.endpoint, targetModel);
        }
      }

      setActiveModel(targetModel);
      const isRemote = active?.kind === "remote" || (targetModel.includes("openrouter") ?? false);
      setIsLocalModel(!isRemote);
    } catch {
      // keep fallback
    }
  }, []);

  useEffect(() => {
    refreshActiveModel();
    core
      .getModelPreferences()
      .then((prefs) => {
        if (prefs.image_model) {
          setActiveImageModel(prefs.image_model);
        } else {
          core
            .listImageModels()
            .then((l) => setActiveImageModel(l[0] ?? ""))
            .catch(() => {});
        }
      })
      .catch(() => {
        core
          .listImageModels()
          .then((l) => setActiveImageModel(l[0] ?? ""))
          .catch(() => {});
      });
  }, [refreshActiveModel]);

  // A chat inside a project works in that folder — reflect it in the picker.
  useEffect(() => {
    if (!projectId) {
      setWorkspace({ kind: "none", id: null, label: "Dossier de travail" });
      return;
    }
    core
      .listProjects()
      .then((ps) => {
        const p = ps.find((x) => x.id === projectId);
        if (p) setWorkspace({ kind: "local", id: p.id, label: p.name, path: p.path });
      })
      .catch(() => {});
  }, [projectId]);

  async function handleWorkspaceChange(next: WorkspaceSelection) {
    setWorkspace(next);
    if (next.kind === "local" && next.id) {
      if (sessionId && projectId !== next.id) {
        try {
          await core.moveSession(sessionId, next.id);
          onSessionMoved?.(next.id);
        } catch (e) {
          console.warn("moveSession failed:", e);
        }
      }
      if (next.path) {
        setRunCwd(next.path);
      } else {
        const ps = await core.listProjects().catch(() => []);
        const found = ps.find((p) => p.id === next.id);
        if (found) setRunCwd(found.path);
      }
    } else if (next.kind === "none") {
      if (sessionId && projectId) {
        try {
          await core.moveSession(sessionId, FREE_CHAT_PATH);
          onSessionMoved?.(FREE_CHAT_PATH);
        } catch (e) {
          console.warn("detach session failed:", e);
        }
      }
      if (sessionId) {
        core
          .sessionWorkspace(sessionId)
          .then(setRunCwd)
          .catch(() => setRunCwd(null));
      }
    }
  }

  // Resolve the real workspace directory for this session (project path or a
  // per-session temp folder for free chats).
  useEffect(() => {
    if (!sessionId) {
      setRunCwd(null);
      return;
    }
    core
      .sessionWorkspace(sessionId)
      .then((p) => setRunCwd(p))
      .catch(() => setRunCwd(null));
  }, [sessionId]);

  /** Ensure the code runner has a cwd before executing. */
  async function resolveRunCwd(): Promise<string | null> {
    if (runCwd) return runCwd;
    if (!sessionId) return null;
    try {
      const p = await core.sessionWorkspace(sessionId);
      setRunCwd(p);
      return p;
    } catch {
      return null;
    }
  }

  // Keep the Documents chip count in sync (refreshes when the panel closes).
  // biome-ignore lint/correctness/useExhaustiveDependencies: `ragOpen` n'est pas lu dans le corps, il sert de déclencheur — refermer le panneau Documents doit relancer le comptage pour que la pastille montre les fragments qui viennent d'être indexés.
  useEffect(() => {
    if (!projectId) {
      setRagCount(0);
      return;
    }
    let cancelled = false;
    core
      .ragStatus(projectId)
      .then((s) => {
        if (!cancelled) setRagCount(s.chunk_count);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [projectId, ragOpen]);

  // Background image generations deliver their finished image into the chat
  // they were requested from. It is also persisted (see imageJobs), so it is
  // still there after switching chats; only append live if this chat is the one.
  useEffect(() => {
    setImageResultHandler((r) => {
      if (r.sessionId && r.sessionId !== sessionId) return;
      setItems((prev) => [
        ...prev,
        {
          id: nextId("msg"),
          kind: "msg",
          role: "assistant",
          text: `${r.simulated ? "(simulation) " : ""}Image générée — « ${r.prompt} »`,
          images: [r.url],
          imagePaths: [r.path],
        },
      ]);
    });
    return () => setImageResultHandler(null);
  }, [sessionId]);

  // External trigger to open image gen panel (from Marketplace / Installed views)
  useEffect(() => {
    if (forceOpenImageGen) {
      setImageGenOpen(true);
    }
  }, [forceOpenImageGen]);

  useEffect(() => {
    if (!sessionId) {
      setItems([]);
      setFollowups([]);
      return;
    }
    setFollowups([]);
    if (skipLoadRef.current === sessionId) {
      skipLoadRef.current = null;
      return;
    }
    (async () => {
      try {
        const msgs = await core.listMessages(sessionId);
        const restored = msgs.flatMap(storedMessageItems);
        setItems(restored);
        for (const item of restored) {
          if (item.kind !== "audio") continue;
          void loadMediaObjectUrl(item.path)
            .then((url) => {
              setItems((prev) =>
                prev.map((current) =>
                  current.kind === "audio" && current.id === item.id
                    ? { ...current, url, status: "ready", error: undefined }
                    : current,
                ),
              );
            })
            .catch((error) => {
              setItems((prev) =>
                prev.map((current) =>
                  current.kind === "audio" && current.id === item.id
                    ? {
                        ...current,
                        status: "error",
                        error: error instanceof Error ? error.message : String(error),
                      }
                    : current,
                ),
              );
            });
        }
      } catch (e) {
        setItems([{ id: nextId("log"), kind: "log", text: `(error loading session: ${e})` }]);
      }
    })();
  }, [sessionId]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `items` et `streaming` ne sont pas lus, ils datent le rendu — c'est justement leur changement qui doit faire redescendre le fil sur le dernier message.
  useEffect(() => {
    if (streamRef.current) {
      streamRef.current.scrollTop = streamRef.current.scrollHeight;
    }
  }, [items, streaming]);

  function autoGrow() {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = "auto";
    const nextH = Math.min(el.scrollHeight, 220);
    el.style.height = `${nextH}px`;
  }

  function handleEvent(ev: StreamEvent) {
    if (ev.type === "token") {
      fullTextRef.current += ev.text;
      setItems((prev) => {
        const last = prev[prev.length - 1];
        if (last && last.kind === "msg" && last.role === "assistant") {
          return [...prev.slice(0, -1), { ...last, text: last.text + ev.text }];
        }
        return [...prev, { id: nextId("msg"), kind: "msg", role: "assistant", text: ev.text }];
      });
    } else if (ev.type === "tool_call") {
      setItems((prev) => [
        ...prev,
        {
          id: nextId("tool"),
          kind: "tool",
          callId: ev.call_id,
          tool: ev.tool,
          args: ev.args,
          status: "running",
          output: "",
        },
      ]);
    } else if (ev.type === "tool_result") {
      setItems((prev) =>
        prev.map((it) => {
          if (it.kind === "tool" && it.callId === ev.call_id) {
            return {
              ...it,
              status: ev.ok ? "ok" : "error",
              output: ev.output,
            };
          }
          return it;
        }),
      );
    } else if (ev.type === "tool_approval") {
      // La boucle d'agent est garée sur un canal en attendant ce verdict.
      // Tant que rien n'est répondu, l'appel n'a pas eu lieu.
      setApproval({
        call_id: ev.call_id,
        tool: ev.tool,
        args: ev.args,
        risk: ev.risk,
        reason: ev.reason,
        diff: ev.diff,
        is_remote: ev.is_remote,
      });
    } else if (ev.type === "artifact" && ev.kind === "audio_wav") {
      const audioId = nextId("audio");
      setItems((prev) => [
        ...prev,
        { id: audioId, kind: "audio", path: ev.path, url: null, status: "loading" },
      ]);
      void loadMediaObjectUrl(ev.path)
        .then((url) => {
          setItems((prev) =>
            prev.map((item) =>
              item.kind === "audio" && item.id === audioId
                ? { ...item, url, status: "ready", error: undefined }
                : item,
            ),
          );
        })
        .catch((error) => {
          setItems((prev) =>
            prev.map((item) =>
              item.kind === "audio" && item.id === audioId
                ? {
                    ...item,
                    status: "error",
                    error: error instanceof Error ? error.message : String(error),
                  }
                : item,
            ),
          );
        });
    } else if (ev.type === "log") {
      setItems((prev) => [...prev, { id: nextId("log"), kind: "log", text: `[Log: ${ev.msg}]` }]);
    }
  }

  async function handleSaveAudio(path: string) {
    const destination = await pickSaveFile("note-vocale.wav", ["wav", "mp3", "ogg", "m4a"]);
    if (!destination) return;
    try {
      await core.saveAudioAs(path, destination);
    } catch (error) {
      setItems((prev) => [
        ...prev,
        {
          id: nextId("log"),
          kind: "log",
          text: `Enregistrement de la note vocale impossible : ${String(error)}`,
        },
      ]);
    }
  }

  /** Transmet la décision, puis referme. Le refus explicite passe par le même
   *  chemin que l'autorisation : c'est la boucle d'agent qui en tire les
   *  conséquences, pas l'interface. */
  async function resolveApproval(decision: ToolApprovalDecision) {
    setApproval(null);
    try {
      await core.approveToolCall(decision);
    } catch (e) {
      // Le délai a pu expirer pendant la lecture du diff : l'appel a alors
      // déjà été refusé côté runtime. On le dit plutôt que de laisser croire
      // que le clic a porté.
      setItems((prev) => [
        ...prev,
        { id: nextId("log"), kind: "log", text: `[Approbation : ${e}]` },
      ]);
    }
  }

  async function onPickFiles(e: React.ChangeEvent<HTMLInputElement>) {
    if (!e.target.files) return;
    const picked = Array.from(e.target.files);
    const read = await Promise.all(picked.map(readFile));
    setAttachments((prev) => [...prev, ...read]);
    e.target.value = "";
  }

  async function send(textOverride?: string, attachOverride?: Attachment[]) {
    // Une commande de plugin tapee en entier doit etre resolue, pas envoyee
    // telle quelle. `textOverride` est ce que la resolution renvoie : le test
    // ne s'applique qu'a la saisie directe, sinon la commande se rappellerait.
    if (textOverride === undefined) {
      const hit = matchExtensionCommand(input.trim(), extCommands);
      if (hit) {
        runSlash(hit);
        return;
      }
    }
    const text = (textOverride ?? input).trim();
    const imgs = attachOverride ?? attachments;

    if (!text && imgs.length === 0) return;

    // Home screen: no chat yet — the first prompt creates one, auto-named after
    // its subject. `skipLoadRef` stops the load effect from wiping what we are
    // about to append optimistically for the brand-new session.
    let sid = sessionId;
    if (!sid) {
      if (!onCreateSessionForPrompt) return;
      const created = await onCreateSessionForPrompt(
        text,
        workspace.kind === "local" ? workspace.id : null,
      );
      if (!created) return;
      sid = created.id;
      skipLoadRef.current = created.id;
    }

    if (streaming) {
      if (!textOverride) {
        setMessageQueue((prev) => [...prev, { id: nextId("q"), text, attachments: [...imgs] }]);
        setInput("");
        setAttachments([]);
        if (inputRef.current) inputRef.current.style.height = "auto";
      }
      return;
    }

    if (!textOverride) {
      setInput("");
      setAttachments([]);
      if (inputRef.current) inputRef.current.style.height = "auto";
    }

    // Show the message and the thinking indicator FIRST. Anything slow (model
    // loading, intent detection) must never leave the user staring at a frozen
    // composer after pressing Enter.
    setStreaming(true);
    setFollowups([]);
    fullTextRef.current = "";
    setItems((prev) => [
      ...prev,
      { id: nextId("msg"), kind: "msg", role: "user", text, images: imgs.map((a) => a.dataUrl) },
    ]);

    // A plain sentence like "je veux une image d'un chat" should reach the image
    // generator, not the chat model. The local pre-filter avoids a model call on
    // every message; the model then confirms and translates the prompt, and the
    // user always gets the final say.
    if (
      !textOverride &&
      text &&
      !text.startsWith("/") &&
      imgs.length === 0 &&
      looksLikeImageRequest(text)
    ) {
      try {
        const intent = await core.detectImageRequest(text);
        if (intent.is_image && intent.english_prompt) {
          setItems((prev) => [
            ...prev,
            { id: nextId("intent"), kind: "intent", intent, model: activeModel, original: text },
          ]);
          setStreaming(false);
          return;
        }
      } catch {
        // Detection unavailable — fall through to a normal answer.
      }
    }

    const t0 = Date.now();

    // /plan → decompose and execute step by step, with a verification loop.
    if (planNext) {
      setPlanNext(false);
      try {
        const ran = await runWorkflow(sid, text, {
          onEvent: handleEvent,
          onStepStart: (i, total, step, attempt) =>
            setItems((prev) => [
              ...prev,
              {
                id: nextId("log"),
                kind: "log",
                text: `\u{1f9e9} \u00c9tape ${i + 1}/${total}${attempt > 1 ? ` (essai ${attempt})` : ""} \u2014 ${step}`,
              },
            ]),
          onDone: (ok, attempts) =>
            setItems((prev) => [
              ...prev,
              {
                id: nextId("log"),
                kind: "log",
                text: ok
                  ? `\u2705 Plan termin\u00e9${attempts > 1 ? ` (essai ${attempts})` : ""}.`
                  : `\u26a0\ufe0f Plan non abouti apr\u00e8s ${attempts - 1} essai(s).`,
              },
            ]),
        });
        if (ran) {
          setStreaming(false);
          return;
        }
      } catch {
        // Planning failed — fall through to a normal answer.
      }
    }

    try {
      const chatStart = t0;
      await core.sendMessage(
        sid,
        text,
        handleEvent,
        imgs.length ? imgs.map((a) => a.base64) : undefined,
        jsonMode === "on" || (jsonMode === "auto" && wantsJson(text))
          ? { type: "json_object" }
          : null,
        reasoningPayload(reasoning),
      );
    } catch (e) {
      setItems((prev) => [...prev, { id: nextId("log"), kind: "log", text: `send failed: ${e}` }]);
    } finally {
      setStreaming(false);

      // Update the duration estimator so it can learn how fast each model
      // produces answers on this machine.
      const durationMs = Date.now() - t0;
      const lastAnswer = fullTextRef.current.trim();
      if (lastAnswer) {
        recordTextGenerationDuration(
          { model: activeModel, reasoning },
          durationMs,
          lastAnswer.length,
        );
      }

      // Invisible side call: ask the model what to do next, shown as chips.
      if (lastAnswer) {
        setFollowupsLoading(true);
        core
          .suggestFollowups(lastAnswer)
          .then((s) => setFollowups(s.slice(0, 3)))
          .catch(() => setFollowups([]))
          .finally(() => setFollowupsLoading(false));
      }

      const currentQ = queueRef.current;
      if (currentQ.length > 0) {
        const nextMsg = currentQ[0];
        setMessageQueue((prev) => prev.slice(1));
        setTimeout(() => {
          send(nextMsg.text, nextMsg.attachments);
        }, 50);
      }
    }
  }

  function removeQueuedMessage(index: number) {
    setMessageQueue((prev) => prev.filter((_, i) => i !== index));
  }

  function editLastUserMessage() {
    for (let i = items.length - 1; i >= 0; i--) {
      const it = items[i];
      if (it.kind === "msg" && it.role === "user") {
        setInput(it.text);
        setItems((prev) => prev.filter((_, idx) => idx !== i));
        inputRef.current?.focus();
        return;
      }
    }
  }

  /** Run a code block from an answer, after explicit confirmation.
   *
   *  Output goes to the side panel rather than the transcript: a build log or
   *  a stack trace inside the conversation buries the discussion it belongs to.
   *  HTML is not executed at all — it is rendered live in a sandboxed frame. */
  async function handleRunCode(code: string, lang: string) {
    if (["html", "htm", "svg"].includes(lang)) {
      showWebRun(code, lang === "svg" ? "Aperçu SVG" : "Aperçu HTML");
      return;
    }

    const runners: Record<string, (f: string) => string> = {
      python: (f) => `python "${f}"`,
      py: (f) => `python "${f}"`,
      javascript: (f) => `node "${f}"`,
      js: (f) => `node "${f}"`,
      node: (f) => `node "${f}"`,
      powershell: (f) => `powershell -File "${f}"`,
      ps1: (f) => `powershell -File "${f}"`,
    };
    const preview = code.length > 400 ? `${code.slice(0, 400)}\u2026` : code;
    if (!window.confirm(`Ex\u00e9cuter ce code (${lang}) sur votre machine ?\n\n${preview}`))
      return;

    const cwd = await resolveRunCwd();
    // Shell snippets run as-is; other languages go through a temp file.
    const isShell = ["bash", "sh", "shell"].includes(lang);
    const cmd = isShell
      ? code
      : (() => {
          const ext = lang.startsWith("py")
            ? "py"
            : lang === "powershell" || lang === "ps1"
              ? "ps1"
              : "js";
          const file = `locaryn_snippet_${Date.now()}.${ext}`;
          const body = code.replace(/'/g, "''");
          const write = `@'\n${body}\n'@ | Set-Content -Encoding utf8 "$env:TEMP\\${file}"`;
          const run = (runners[lang] ?? runners.python)(`$env:TEMP\\${file}`);
          return `powershell -NoProfile -Command "${write.replace(/"/g, '\\"')}"; ${run}`;
        })();

    // A session with no workspace still runs, just wherever the shell starts;
    // say so rather than showing an empty path.
    startTerminalRun(lang, isShell ? code : `${lang} (extrait)`, cwd ?? "dossier par défaut");
    try {
      await core.runTerminal(cmd, cwd, (ev) => {
        if (ev.type === "line") appendRunLine(ev.text);
        else if (ev.type === "exit") finishTerminalRun(ev.code ?? 0);
      });
      finishTerminalRun(0);
    } catch (e) {
      appendRunLine(String(e));
      finishTerminalRun(1);
    }
  }

  useEffect(() => {
    let cancelled = false;
    core
      .listExtensionCommands()
      .then((cmds) => {
        if (cancelled) return;
        setExtCommands(
          cmds.map((c) => ({
            name: c.name,
            aliases: [c.name.split(":").pop() ?? c.name],
            icon: "\u{1F9E9}",
            label: c.description ?? c.name,
            hint: `Commande de ${c.plugin}`,
            action: "extension" as const,
            extension: c.name,
          })),
        );
      })
      .catch(() => setExtCommands([]));
    return () => {
      cancelled = true;
    };
  }, []);

  function runSlash(cmd: SlashCommand, size?: number | null) {
    // Capture avant vidage : une commande de plugin a besoin de ce qui a ete
    // tape apres son nom pour resoudre ses arguments.
    const raw = input.trim();
    setInput("");
    setSlash(null);
    setSlashSize(size ?? null);
    if (inputRef.current) inputRef.current.style.height = "auto";
    switch (cmd.action) {
      case "image":
        setImageGenOpen(true);
        break;
      case "edit-image":
        setImageGenOpen(true);
        break;
      case "documents":
        if (projectId) setRagOpen(true);
        break;
      case "json":
        setJsonMode((v) => (v === "auto" ? "on" : v === "on" ? "off" : "auto"));
        break;
      case "reasoning-off":
        setReasoning("off");
        break;
      case "reasoning-high":
        setReasoning("high");
        break;
      case "model":
        setQuickModelOpen(true);
        break;
      case "settings":
        onOpenSettings?.();
        break;
      case "new-chat":
        onNewChat?.();
        break;
      case "plan":
        setPlanNext(true);
        break;
      case "clear":
        setItems([]);
        setFollowups([]);
        break;
      case "extension": {
        const name = cmd.extension;
        if (!name) break;
        // Tout ce qui suit le nom de la commande devient ses arguments.
        const typed = raw.startsWith("/") ? raw.slice(1) : raw;
        const space = typed.indexOf(" ");
        const args = space >= 0 ? typed.slice(space + 1).trim() : "";
        core
          .resolveExtensionCommand(name, args)
          .then((prompt) => {
            const body = prompt.trim();
            if (body) {
              send(body);
            } else {
              setItems((prev) => [
                ...prev,
                {
                  id: nextId("log"),
                  kind: "log",
                  text: `La commande /${name} n'a produit aucun texte.`,
                },
              ]);
            }
          })
          .catch((e) => {
            // Une commande cassee doit se voir : echouer en silence laisserait
            // croire que rien n'a ete tape.
            setItems((prev) => [
              ...prev,
              { id: nextId("log"), kind: "log", text: `Commande /${name} : ${String(e)}` },
            ]);
          });
        break;
      }
    }
  }

  function handleComposerKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    // Slash palette takes over the arrows / Enter / Escape while it is open.
    if (slash && slash.items.length > 0) {
      const n = slash.items.length;
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSlashIndex((i) => (i + 1) % n);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSlashIndex((i) => (i - 1 + n) % n);
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        const idx = Math.min(slashIndex, n - 1);
        if (slash.kind === "commands") {
          const cmd = slash.items[idx];
          // A command that takes arguments: Tab completes to "/cmd " so its
          // values show up instead of running with an unknown quality.
          if (e.key === "Tab" && cmd.args && cmd.args.length) {
            const next = `/${cmd.name} `;
            setInput(next);
            setSlash(matchSlashInput(next, extCommands, activeCapabilities));
            setSlashIndex(0);
            return;
          }
          runSlash(cmd);
        } else {
          runSlash(slash.command, argToSize(slash.items[idx].value));
        }
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        setSlash(null);
        return;
      }
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
      return;
    }
    if (e.key === "ArrowUp" && input.trim() === "" && !streaming) {
      e.preventDefault();
      editLastUserMessage();
    }
  }

  const lastUserIdx = (() => {
    for (let i = items.length - 1; i >= 0; i--) {
      const it = items[i];
      if (it.kind === "msg" && it.role === "user") return i;
    }
    return -1;
  })();

  const empty = items.length === 0 && !streaming;
  // On the home screen there is no session yet — sending creates one.
  const canCompose = !!sessionId || !!onCreateSessionForPrompt;
  const canSend = canCompose && (!!input.trim() || attachments.length > 0);

  const sendRef = useRef(send);
  sendRef.current = send;

  useEffect(() => {
    pluginBridge.registerChatContext(
      () => input,
      (txt) => {
        setInput(txt);
        if (inputRef.current) {
          inputRef.current.style.height = "auto";
          inputRef.current.style.height = `${Math.min(inputRef.current.scrollHeight, 240)}px`;
        }
      },
      () => {
        if (canSend) sendRef.current();
      },
    );
    return () => {
      pluginBridge.unregisterChatContext();
    };
  }, [input, canSend]);

  // Context calculations
  const usedTokens = items.reduce((acc, it) => {
    if (it.kind === "msg") return acc + Math.ceil(it.text.length / 4);
    if (it.kind === "tool") return acc + Math.ceil((it.output?.length ?? 0) / 4);
    return acc;
  }, 0);
  const ctxWindow = ctxSize || DEFAULT_MODEL_PARAMS.ctx_size;
  const ctxPct = Math.min(usedTokens / ctxWindow, 1);
  const ctxWarnLevel = ctxPct > 0.85 ? "danger" : ctxPct > 0.6 ? "warn" : "ok";

  function ctxFmt(n: number) {
    return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : `${n}`;
  }

  async function handleSwitchModel(tag: string) {
    const providers = await core.listProviders();
    const active = providers.find((p) => p.is_active) ?? providers[0];
    if (active) {
      await core.configureProvider(active.endpoint, tag);
      await refreshActiveModel();
    }
  }

  return (
    <section className={`locaryn-chat${ephemeral ? " locaryn-chat-ephemeral" : ""}`}>
      {/*
        Le mode éphémère se voit, il ne se devine pas. Une bande en tête et un
        liseré tout autour : on doit savoir sans le chercher que ce qui se dit
        ici ne sera pas gardé.
      */}
      {ephemeral && (
        <div className="locaryn-ephemeral-banner">
          Conversation éphémère — rien n'en sera gardé : ni le fil, ni son titre.
        </div>
      )}
      <QuickModelSelector
        isOpen={quickModelOpen}
        onClose={() => setQuickModelOpen(false)}
        activeModel={activeModel}
        installedModels={installedModels}
        isProviderLocal={isLocalModel}
        onSelectModel={handleSwitchModel}
        onOpenMarketplace={onOpenMarketplace}
      />

      <div className="locaryn-chat-scroll" ref={streamRef}>
        {empty ? (
          <div className="locaryn-empty">
            <div className="locaryn-empty-title">Assistant IA Locaryn</div>
            <div className="locaryn-empty-sub">
              Locaryn peut lire, chercher, éditer et exécuter du code dans ce projet ou en chat
              libre.
            </div>
            <div className="locaryn-empty-suggestions">
              {SUGGESTIONS.map((s) => (
                <button
                  key={s}
                  type="button"
                  className="locaryn-suggestion"
                  disabled={!canCompose}
                  onClick={() => send(s)}
                >
                  {s}
                </button>
              ))}
            </div>
          </div>
        ) : (
          <div className="locaryn-chat-column">
            {items.map((it, i) => {
              if (it.kind === "msg") {
                return (
                  // Une entrée en cascade, comme sur le web et le téléphone :
                  // un historique chargé arrive d'un bloc, chaque message entre
                  // un court instant après le précédent.
                  <div
                    key={it.id}
                    className="locaryn-msg-anim"
                    style={{ animationDelay: `${Math.min(i * 35, 350)}ms` }}
                  >
                    <MessageBubble
                      role={it.role}
                      text={it.text}
                      images={it.images}
                      imagePaths={it.imagePaths}
                      canEdit={i === lastUserIdx && !streaming}
                      onEdit={editLastUserMessage}
                      onRunCode={it.role === "assistant" ? handleRunCode : undefined}
                    />
                  </div>
                );
              }
              if (it.kind === "intent") {
                return (
                  <ImageIntentCard
                    key={it.id}
                    intent={it.intent}
                    model={it.model}
                    decided={it.decided}
                    onAccept={(quality) => {
                      const px = IMAGE_QUALITIES.find((q) => q.id === quality)?.px;
                      setItems((prev) =>
                        prev.map((x, j) =>
                          j === i && x.kind === "intent" ? { ...x, decided: "accepted" } : x,
                        ),
                      );
                      void (async () => {
                        const info = await core
                          .appInfo()
                          .catch(() => ({ data_dir: "C:/Users/Public" }));
                        startImageGeneration({
                          model: activeImageModel || it.model,
                          prompt: it.intent.english_prompt,
                          outputDir: `${info.data_dir}/generated_images`,
                          width: px,
                          height: px,
                          sessionId,
                        });
                      })();
                    }}
                    onRefuse={() => {
                      setItems((prev) =>
                        prev.map((x, j) =>
                          j === i && x.kind === "intent" ? { ...x, decided: "refused" } : x,
                        ),
                      );
                      // Answer the original message as a normal chat turn.
                      send(it.original);
                    }}
                  />
                );
              }
              if (it.kind === "audio") {
                return (
                  <VoiceNote
                    key={it.id}
                    url={it.url}
                    status={it.status}
                    error={it.error}
                    onSave={() => handleSaveAudio(it.path)}
                  />
                );
              }
              if (it.kind === "tool") {
                return (
                  <ToolCard
                    key={it.id}
                    tool={it.tool}
                    args={it.args}
                    status={it.status}
                    output={it.output}
                  />
                );
              }
              return (
                <div key={it.id} className="locaryn-chat-log">
                  {it.text}
                </div>
              );
            })}
            {streaming && (
              <div className="locaryn-thinking" aria-live="polite">
                <span className="locaryn-thinking-dot" />
                <span className="locaryn-thinking-dot" />
                <span className="locaryn-thinking-dot" />
              </div>
            )}

            {/* Next-step suggestions, asked to the model in the background. */}
            {!streaming && (followupsLoading || followups.length > 0) && (
              <div className="locaryn-followups">
                <span className="locaryn-followups-label">
                  {followupsLoading ? "Suggestions…" : "Et ensuite ?"}
                </span>
                {followups.map((f) => (
                  <button
                    key={f}
                    type="button"
                    className="locaryn-followup"
                    onClick={() => {
                      // A suggestion may be a slash command (e.g. "/image brouillon"
                      // for a throwaway icon) — run it instead of sending it as text.
                      if (f.startsWith("/")) {
                        const sug = matchSlashInput(f.trim(), extCommands, activeCapabilities);
                        if (sug?.kind === "args") {
                          runSlash(sug.command, argToSize(sug.items[0].value));
                          return;
                        }
                        if (sug?.kind === "commands" && sug.items.length) {
                          runSlash(sug.items[0]);
                          return;
                        }
                      }
                      send(f);
                    }}
                    title={f.startsWith("/") ? "Exécuter cette action" : "Envoyer cette suite"}
                  >
                    {f}
                  </button>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      <div className="locaryn-composer">
        {/* Message queue visual display */}
        {messageQueue.length > 0 && (
          <div className="locaryn-queue-container">
            <div className="locaryn-queue-title">File d'attente ({messageQueue.length})</div>
            {messageQueue.map((qm, i) => (
              <div key={qm.id} className="locaryn-queue-item">
                <span className="locaryn-queue-text">{qm.text || "(Image seule)"}</span>
                <button
                  type="button"
                  className="locaryn-queue-remove"
                  onClick={() => removeQueuedMessage(i)}
                  title="Retirer de la file d'attente"
                >
                  <Icon name="close" size={13} />
                </button>
              </div>
            ))}
          </div>
        )}

        {/* Slash-command palette (type "/" in the composer) */}
        {slash && slash.items.length > 0 && (
          // Une palette de commandes n'a pas d'équivalent HTML natif : `select`
          // n'accepte ni la saisie libre qui la filtre, ni le contenu riche de
          // chaque ligne. Le motif listbox/option d'ARIA est la réponse prévue
          // pour ce cas. `tabIndex={-1}` parce que le focus ne quitte jamais le
          // champ de saisie — ce sont ses flèches qui déplacent la sélection,
          // et un arrêt de tabulation ici piégerait l'utilisateur.
          <div className="locaryn-slash" role="listbox" aria-label="Commandes" tabIndex={-1}>
            {slash.kind === "args" && (
              <div className="locaryn-slash-head">
                {slash.command.icon} {slash.command.label} — choisissez la qualite
              </div>
            )}
            {slash.kind === "commands"
              ? slash.items.map((c, i) => (
                  <button
                    key={c.name}
                    type="button"
                    role="option"
                    aria-selected={i === slashIndex}
                    className={`locaryn-slash-item${i === slashIndex ? " locaryn-active" : ""}`}
                    onMouseEnter={() => setSlashIndex(i)}
                    onClick={() => runSlash(c)}
                  >
                    <span className="locaryn-slash-icon">{c.icon}</span>
                    <span className="locaryn-slash-text">
                      <span className="locaryn-slash-label">{c.label}</span>
                      <span className="locaryn-slash-hint">{c.hint}</span>
                    </span>
                    <code className="locaryn-slash-cmd">
                      /{c.name}
                      {c.args?.length ? " ..." : ""}
                    </code>
                  </button>
                ))
              : slash.items.map((a, i) => (
                  <button
                    key={a.value}
                    type="button"
                    role="option"
                    aria-selected={i === slashIndex}
                    className={`locaryn-slash-item${i === slashIndex ? " locaryn-active" : ""}`}
                    onMouseEnter={() => setSlashIndex(i)}
                    onClick={() => runSlash(slash.command, argToSize(a.value))}
                  >
                    <span className="locaryn-slash-icon">&#128208;</span>
                    <span className="locaryn-slash-text">
                      <span className="locaryn-slash-label">{a.label}</span>
                      <span className="locaryn-slash-hint">{a.hint}</span>
                    </span>
                    <code className="locaryn-slash-cmd">{a.value}</code>
                  </button>
                ))}
          </div>
        )}

        <ProjectSuggestion
          sessionId={sessionId}
          projectId={projectId}
          messageCount={items.filter((i) => i.kind === "msg").length}
          ephemeral={ephemeral}
          onMoved={onSessionMoved}
        />

        <div className="locaryn-composer-context">
          <WorkspacePicker
            value={workspace}
            onChange={handleWorkspaceChange}
            onAddProject={onAddProject}
            onAddSsh={onAddSsh}
            cloudConnected={connectionMode === "remote"}
          />
        </div>

        <div className="locaryn-composer-card">
          {attachments.length > 0 && (
            <div className="locaryn-attach-strip">
              {attachments.map((a, i) => (
                <div key={a.id} className="locaryn-attach-chip">
                  <img src={a.dataUrl} alt={a.name} />
                  <button
                    type="button"
                    className="locaryn-attach-remove"
                    aria-label="Retirer l'image"
                    onClick={() => setAttachments((prev) => prev.filter((_, idx) => idx !== i))}
                  >
                    <Icon name="close" size={13} />
                  </button>
                </div>
              ))}
            </div>
          )}
          <textarea
            ref={inputRef}
            className="locaryn-composer-input"
            rows={1}
            placeholder={
              sessionId ? "Posez votre question à Locaryn…" : "Sélectionnez ou créez une session"
            }
            value={input}
            disabled={!canCompose}
            onChange={(e) => {
              setInput(e.target.value);
              autoGrow();
              setSlash(matchSlashInput(e.target.value, extCommands, activeCapabilities));
              setSlashIndex(0);
            }}
            onKeyDown={handleComposerKeyDown}
          />
          <div className="locaryn-composer-bar">
            {/* Slot toolbar pour les extensions (boutons personnalisés, raccourcis) */}
            <ExtensionSlot
              name="composer.toolbar"
              context={{ input, setInput, send, canCompose }}
            />

            <input
              ref={fileRef}
              type="file"
              accept="image/*"
              multiple
              hidden
              onChange={onPickFiles}
            />
            {/* Action chips — each states what it does and whether it's ON. */}
            <button
              type="button"
              className="locaryn-chip-btn"
              title="Joindre une image au message"
              disabled={!canCompose}
              onClick={() => fileRef.current?.click()}
            >
              <Icon name="plus" size={15} /> Joindre
            </button>
            <ReasoningPicker value={reasoning} onChange={setReasoning} disabled={!canCompose} />

            <button
              type="button"
              className={`locaryn-chip-btn${ragCount > 0 ? " locaryn-chip-on" : ""}`}
              title="Base de connaissances : ajoutez des documents, le modèle s'en servira pour répondre"
              disabled={!projectId}
              onClick={() => setRagOpen(true)}
            >
              <Icon name="models" size={15} /> Documents
              {ragCount > 0 && <span className="locaryn-chip-state">{ragCount}</span>}
            </button>
            {composerError && (
              <span
                className="locaryn-composer-hint"
                style={{ color: "var(--danger)", flex: "none" }}
                title={composerError}
              >
                {composerError.length > 60 ? `${composerError.slice(0, 57)}…` : composerError}
              </span>
            )}
            <span className="locaryn-composer-hint">
              Entrée pour envoyer · Shift+Entrée pour saut de ligne
            </span>

            {/* Slot avant envoi (ex: dictaphone / micro, actions rapides de validation) */}
            <ExtensionSlot
              name="composer.before_send"
              context={{ input, setInput, send, canCompose }}
            />

            <button
              type="button"
              className="locaryn-send"
              onClick={() => send()}
              disabled={!canSend}
              aria-label="Envoyer le message"
            >
              {streaming ? "File +1 ↵" : "Envoyer ↵"}
            </button>
          </div>

          {/* Context gauge & In-Chat Model Quick Switcher Bar */}
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              borderTop: "1px solid var(--border)",
              paddingTop: "4px",
              paddingRight: "4px",
            }}
          >
            {/* Noyau alternatif : le modèle Locaryn n'a pas la main sur cette
                conversation — le badge le dit, à la place du sélecteur. */}
            {coreName ? (
              <span
                style={{
                  fontSize: "11px",
                  display: "flex",
                  alignItems: "center",
                  gap: "6px",
                  padding: "2px 8px",
                  color: "var(--text)",
                  background: "var(--bg)",
                  border: "1px solid var(--accent)",
                  borderRadius: "var(--radius-xs)",
                }}
                title="Cette conversation est confiée à un noyau alternatif : sa mémoire, ses skills et son fournisseur lui appartiennent."
              >
                <span style={{ display: "inline-flex" }}>
                  <Icon name="extensions" size={14} />
                </span>
                <span style={{ fontWeight: 700 }}>Noyau : {coreName}</span>
              </span>
            ) : (
              /* Model Quick Picker pill */
              <button
                type="button"
                className="locaryn-btn-ghost"
                style={{
                  fontSize: "11px",
                  display: "flex",
                  alignItems: "center",
                  gap: "6px",
                  padding: "2px 8px",
                  color: "var(--text)",
                  background: "var(--bg)",
                  border: "1px solid var(--border)",
                  borderRadius: "var(--radius-xs)",
                }}
                onClick={() => {
                  refreshActiveModel();
                  setQuickModelOpen(true);
                }}
                title="Changer rapidement parmi vos modèles installés"
              >
                <span style={{ display: "inline-flex" }}>
                  <Icon name={isLocalModel ? "cpu" : "cloud"} size={14} />
                </span>
                <span style={{ fontWeight: 700 }}>{activeModel || "Choisir un modèle"}</span>
                <span style={{ fontSize: "9px", color: "var(--text-faint)" }}>▾</span>
              </button>
            )}

            {/* Context gauge */}
            <div
              className="locaryn-ctx-gauge-wrap"
              title={`~${ctxFmt(usedTokens)} / ${ctxFmt(ctxWindow)} tokens utilisés`}
            >
              <div
                className={`locaryn-ctx-gauge-bar locaryn-ctx-gauge-${ctxWarnLevel}`}
                style={{ width: `${ctxPct * 100}%` }}
              />
              <span className="locaryn-ctx-gauge-label">
                <span className={`locaryn-ctx-gauge-used locaryn-ctx-gauge-text-${ctxWarnLevel}`}>
                  ~{ctxFmt(usedTokens)}
                </span>
                <span className="locaryn-ctx-gauge-sep">/</span>
                <span className="locaryn-ctx-gauge-total">{ctxFmt(ctxWindow)} ctx</span>
              </span>
            </div>
          </div>
        </div>
      </div>

      {imageGenOpen && (
        <ImageGenPanel
          installedModels={installedModels}
          sessionId={sessionId}
          forcedSize={slashSize}
          onClose={() => {
            setImageGenOpen(false);
            setSlashSize(null);
            onImageGenClosed?.();
          }}
          onImageGenerated={(path) => {
            const url = toImageUrl(path);
            setItems((prev) => [
              ...prev,
              {
                id: nextId("msg"),
                kind: "msg",
                role: "assistant",
                text: `Image générée localement :\n\n![Image](${url})`,
                images: [url],
              },
            ]);
          }}
          onInstallRequested={async (tag, consent) => {
            const providers = await core.listProviders();
            const active = providers.find((p) => p.is_active) ?? providers[0];
            if (active) {
              await core.pullModel(active.endpoint, tag, undefined, undefined, consent);
              await refreshActiveModel();
            }
          }}
        />
      )}

      {ragOpen && projectId && <RagPanel projectId={projectId} onClose={() => setRagOpen(false)} />}

      {/* Approbation d'un appel d'outil. La boucle d'agent attend ce verdict :
          fermer sans répondre vaut refus, et le runtime finit par se lasser
          de son côté plutôt que d'attendre indéfiniment. */}
      <ToolApprovalModal
        approval={approval}
        onResolve={(decision) => void resolveApproval(decision)}
        onCancel={() => {
          if (!approval) return;
          void resolveApproval({
            call_id: approval.call_id,
            tool: approval.tool,
            risk: approval.risk,
            decision: "deny",
            scope: "once",
            note: null,
          });
        }}
      />
    </section>
  );
}
