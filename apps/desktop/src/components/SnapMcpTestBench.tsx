import { useCallback, useEffect, useRef, useState } from "react";
import {
  type AndroidScreenProbe,
  type InstalledExtension,
  type McpServerInfo,
  type SnapMcpDiagnostics,
  core,
} from "../lib/core";
import { pickAnyFile } from "../lib/dialog";

type Backend = "mock" | "web" | "adb" | "appium" | "telegram";
type MediaVisibility = "saved" | "timed_10s" | "view_once" | "view_once_replay";

type RunResult = {
  title: string;
  value: unknown;
};

const BACKENDS: { value: Backend; label: string; note: string }[] = [
  { value: "mock", label: "Simulation", note: "Aucun compte et aucun envoi réel." },
  { value: "telegram", label: "Telegram", note: "Compte personnel via MTProto." },
  { value: "web", label: "Snapchat Web", note: "Session Playwright sur web.snapchat.com." },
  {
    value: "adb",
    label: "Snapchat Android — ADB léger",
    note: "Téléphone connecté avec le pilote minimal.",
  },
  {
    value: "appium",
    label: "Snapchat Android — Appium",
    note: "Lecture UI sémantique via UiAutomator2.",
  },
];

type SetupStep = {
  id: string;
  phase: string;
  title: string;
  detail: string;
  checks?: string[];
};

const SETUP_STORAGE_KEY = "locaryn:snapmcp-setup-checklist";
const SETUP_STEPS: SetupStep[] = [
  {
    id: "pc-runtime",
    phase: "PC",
    title: "Installer Node.js",
    detail: "Node.js 20 ou plus récent doit être disponible dans le PATH.",
    checks: ["node"],
  },
  {
    id: "pc-audio",
    phase: "PC",
    title: "Installer ffmpeg",
    detail: "Nécessaire pour convertir les vocaux du micro PC en OGG/Opus Telegram.",
    checks: ["ffmpeg"],
  },
  {
    id: "pc-browser",
    phase: "PC",
    title: "Installer Chromium Playwright",
    detail: "Nécessaire pour Snapchat Web et son parcours QR.",
    checks: ["chromium"],
  },
  {
    id: "telegram-account",
    phase: "Telegram",
    title: "Créer api_id et api_hash",
    detail: "Sur my.telegram.org → API development tools. Aucun bot Telegram requis.",
    checks: ["telegram_credentials"],
  },
  {
    id: "telegram-session",
    phase: "Telegram",
    title: "Créer la session du compte personnel",
    detail: "Lancer npm run telegram:login, puis saisir numéro, code et mot de passe 2FA.",
    checks: ["telegram_session"],
  },
  {
    id: "adb-tools",
    phase: "Android",
    title: "Installer Android Platform Tools",
    detail: "La commande adb doit être accessible depuis le PATH.",
    checks: ["adb"],
  },
  {
    id: "android-device",
    phase: "Android",
    title: "Autoriser le téléphone Android",
    detail: "Activer le débogage USB, déverrouiller le téléphone et accepter la clé RSA.",
    checks: ["android_device"],
  },
  {
    id: "android-vm",
    phase: "Android VM",
    title: "Installer l’AVD SnapMCP",
    detail: "Installer l’image Google APIs légère et créer SnapMcpVM depuis le bouton VM.",
  },
  {
    id: "snapchat-web",
    phase: "Snapchat",
    title: "Connecter Snapchat Web",
    detail: "Ouvrir le navigateur, scanner le QR code, puis vérifier la connexion.",
  },
  {
    id: "mcp-server",
    phase: "Finalisation",
    title: "Vérifier le serveur MCP",
    detail: "Le serveur SnapMCP doit être enregistré et ses outils doivent être découverts.",
    checks: ["mcp_servers"],
  },
];

function extensionConfig(backend: Backend): Record<string, unknown> {
  if (backend === "telegram") return { "transport.driver": "telegram" };
  return {
    "transport.driver": "snapchat",
    "transport.snapchat_client": backend,
  };
}

function pretty(value: unknown): string {
  return JSON.stringify(value, null, 2);
}

export function SnapMcpTestBench() {
  const [backend, setBackend] = useState<Backend>("mock");
  const [conversationId, setConversationId] = useState("conv_1");
  const [text, setText] = useState("Bonjour depuis Locaryn.");
  const [mediaUrl, setMediaUrl] = useState("https://picsum.photos/seed/snapmcp-test/640/960");
  const [mediaType, setMediaType] = useState<"image" | "video">("image");
  const [visibility, setVisibility] = useState<MediaVisibility>("saved");
  const [voicePath, setVoicePath] = useState("");
  const [voiceMime, setVoiceMime] = useState("audio/ogg");
  const [extensions, setExtensions] = useState<InstalledExtension[]>([]);
  const [servers, setServers] = useState<McpServerInfo[]>([]);
  const [serverName, setServerName] = useState("");
  const [configuredBackend, setConfiguredBackend] = useState<Backend | null>(null);
  const [diagnostics, setDiagnostics] = useState<SnapMcpDiagnostics | null>(null);
  const [vmStatus, setVmStatus] = useState<import("../lib/core").AndroidVmStatus | null>(null);
  const [vmBusy, setVmBusy] = useState(false);
  const [screenProbe, setScreenProbe] = useState<AndroidScreenProbe | null>(null);
  const [screenBusy, setScreenBusy] = useState(false);
  const [screenX, setScreenX] = useState("540");
  const [screenY, setScreenY] = useState("900");
  const [screenX2, setScreenX2] = useState("540");
  const [screenY2, setScreenY2] = useState("500");
  const [setupChecked, setSetupChecked] = useState<Record<string, boolean>>({});
  const [result, setResult] = useState<RunResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [recording, setRecording] = useState(false);
  const [activeCallId, setActiveCallId] = useState("");
  const recorderRef = useRef<MediaRecorder | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);

  const refresh = useCallback(async () => {
    try {
      const [nextServers, nextExtensions] = await Promise.all([
        core.listMcpServers(),
        core.listExtensions(),
      ]);
      const selected =
        nextServers.find((item) => /snapmcp|snap-astreinte/i.test(item.name)) ?? nextServers[0];
      setServers(nextServers);
      setExtensions(nextExtensions);
      setServerName(selected?.name ?? "");
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    return () => {
      recorderRef.current?.stop();
      for (const track of streamRef.current?.getTracks() ?? []) track.stop();
    };
  }, [refresh]);

  useEffect(() => {
    try {
      const saved = localStorage.getItem(SETUP_STORAGE_KEY);
      if (saved) setSetupChecked(JSON.parse(saved) as Record<string, boolean>);
    } catch {
      // La checklist reste utilisable si le stockage du navigateur est indisponible.
    }
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(SETUP_STORAGE_KEY, JSON.stringify(setupChecked));
    } catch {
      // Le suivi manuel est seulement un confort ; le diagnostic reste la source de vérité.
    }
  }, [setupChecked]);

  const backendInfo = BACKENDS.find((item) => item.value === backend)!;
  const installed = extensions.some((item) => /snap-astreinte/i.test(item.name));
  const serverReady = servers.some((item) => item.name === serverName && item.running);

  async function chooseMedia() {
    const path = await pickAnyFile("Image ou vidéo", [
      "png",
      "jpg",
      "jpeg",
      "webp",
      "gif",
      "mp4",
      "mov",
      "webm",
    ]);
    if (path) setMediaUrl(path);
  }

  async function chooseAudio() {
    const path = await pickAnyFile("Audio", ["ogg", "opus", "webm", "wav", "mp3", "m4a"]);
    if (!path) return;
    setVoicePath(path);
    setVoiceMime(
      path.toLowerCase().endsWith(".ogg") || path.toLowerCase().endsWith(".opus")
        ? "audio/ogg"
        : "audio/webm",
    );
  }

  async function startRecording() {
    if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
      setError("L'enregistrement micro n'est pas disponible dans cette fenêtre.");
      return;
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const mime =
        ["audio/ogg;codecs=opus", "audio/webm;codecs=opus", "audio/webm"].find((candidate) =>
          MediaRecorder.isTypeSupported(candidate),
        ) ?? "";
      const recorder = new MediaRecorder(stream, mime ? { mimeType: mime } : undefined);
      chunksRef.current = [];
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) chunksRef.current.push(event.data);
      };
      recorder.onstop = async () => {
        try {
          const blob = new Blob(chunksRef.current, { type: recorder.mimeType || "audio/webm" });
          const dataUrl = await new Promise<string>((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = () => resolve(String(reader.result));
            reader.onerror = () => reject(reader.error ?? new Error("lecture audio impossible"));
            reader.readAsDataURL(blob);
          });
          const path = await core.writeTestAudio(dataUrl, blob.type);
          setVoicePath(path);
          setVoiceMime(blob.type || "audio/webm");
          setResult({
            title: "Enregistrement terminé",
            value: { path, mimeType: blob.type, bytes: blob.size },
          });
        } catch (e) {
          setError(String(e));
        } finally {
          for (const track of stream.getTracks()) track.stop();
          streamRef.current = null;
        }
      };
      streamRef.current = stream;
      recorderRef.current = recorder;
      recorder.start();
      setRecording(true);
      setError(null);
    } catch (e) {
      setError(`Microphone refusé ou indisponible : ${String(e)}`);
    }
  }

  function stopRecording() {
    recorderRef.current?.stop();
    recorderRef.current = null;
    setRecording(false);
  }

  async function configureAndStart(selectedBackend = backend): Promise<string> {
    if (serverName && configuredBackend === selectedBackend) return serverName;
    const extension = extensions.find((item) => /snap-astreinte/i.test(item.name));
    if (!extension) {
      throw new Error("Installe d'abord l'extension snap-astreinte depuis Réglages → Extensions.");
    }
    await core.setExtensionConfig(extension.id, extensionConfig(selectedBackend));
    await core.reloadExtensions();
    const available = await core.listMcpServers();
    const selected =
      available.find((item) => /snapmcp|snap-astreinte/i.test(item.name)) ?? available[0];
    if (!selected) throw new Error("Serveur SnapMCP introuvable. Recharge l'extension.");
    if (!selected.running) await core.startMcpServer(selected.name);
    setServers(
      available.map((item) => (item.name === selected.name ? { ...item, running: true } : item)),
    );
    setServerName(selected.name);
    setConfiguredBackend(selectedBackend);
    return selected.name;
  }

  async function invoke(
    title: string,
    tool: string,
    args: Record<string, unknown> = {},
    selectedBackend = backend,
  ) {
    setBusy(true);
    setError(null);
    try {
      const name = await configureAndStart(selectedBackend);
      const value = await core.invokeMcpTool(name, tool, args);
      setResult({ title, value });
      await refresh();
      return value;
    } catch (e) {
      setError(String(e));
      return null;
    } finally {
      setBusy(false);
    }
  }

  async function openSnapchatWebLogin() {
    setBackend("web");
    await invoke("Fenêtre Snapchat Web ouverte", "open_web_login", {}, "web");
  }

  async function verifySnapchatWebLogin() {
    setBackend("web");
    const verified = await invoke(
      "Connexion Snapchat Web vérifiée",
      "web_session_status",
      {},
      "web",
    );
    if (verified !== null) setSetupChecked((current) => ({ ...current, "snapchat-web": true }));
  }

  async function runDiagnostics() {
    setBusy(true);
    setError(null);
    try {
      setDiagnostics(await core.diagnoseSnapMcp());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function refreshVm() {
    setVmBusy(true);
    setError(null);
    try {
      setVmStatus(await core.diagnoseAndroidVm());
    } catch (e) {
      setError(String(e));
    } finally {
      setVmBusy(false);
    }
  }

  async function setupVm() {
    setVmBusy(true);
    setError(null);
    try {
      const status = await core.setupAndroidVm({
        avdName: "SnapMcpVM",
        apiLevel: 35,
        installComponents: true,
      });
      setVmStatus(status);
      setSetupChecked((current) => ({ ...current, "android-vm": true }));
    } catch (e) {
      setError(String(e));
    } finally {
      setVmBusy(false);
    }
  }

  async function startVm() {
    setVmBusy(true);
    setError(null);
    try {
      const avd = vmStatus?.avds[0] ?? vmStatus?.recommendedAvd ?? "SnapMcpVM";
      setVmStatus(
        await core.startAndroidVm({
          avdName: avd,
          memoryMb: 2048,
          camera: "webcam0",
          microphone: "system",
        }),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setVmBusy(false);
    }
  }

  async function stopVm() {
    setVmBusy(true);
    setError(null);
    try {
      setVmStatus(await core.stopAndroidVm());
      setScreenProbe(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setVmBusy(false);
    }
  }

  async function probeScreen(withOcr = true) {
    setScreenBusy(true);
    setError(null);
    try {
      const serial = vmStatus?.runningEmulators[0];
      setScreenProbe(await core.androidScreenProbe({ serial, ocr: withOcr }));
    } catch (e) {
      setError(String(e));
    } finally {
      setScreenBusy(false);
    }
  }

  async function screenAction(action: import("../lib/core").AndroidScreenActionArgs["action"]) {
    setScreenBusy(true);
    setError(null);
    try {
      const serial = screenProbe?.serial ?? vmStatus?.runningEmulators[0];
      setScreenProbe(
        await core.androidScreenAction({
          serial,
          action,
          x: Number(screenX),
          y: Number(screenY),
          x2: Number(screenX2),
          y2: Number(screenY2),
          durationMs: 450,
          ocr: false,
        }),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setScreenBusy(false);
    }
  }

  async function sendMedia() {
    if (visibility !== "saved" && backend !== "telegram") {
      setError("Les modes éphémères sont disponibles uniquement avec Telegram.");
      return;
    }
    if (visibility !== "saved" && mediaType !== "image") {
      setError("Les modes éphémères Telegram sont limités aux photos.");
      return;
    }
    await invoke("Média envoyé", "send_snap", {
      conversationId,
      mediaUrl,
      type: mediaType,
      visibility,
    });
  }

  async function sendVoice() {
    if (!voicePath && !text.trim()) {
      setError("Enregistre un vocal, choisis un fichier audio ou écris un texte.");
      return;
    }
    const recordedPath = voicePath;
    const sent = await invoke("Vocal envoyé", "send_voice_note", {
      conversationId,
      audioPath: recordedPath || undefined,
      text: text.trim() || undefined,
      language: "fr-FR",
    });
    if (sent !== null && recordedPath.includes("snapmcp-test-")) {
      await core.removeTestAudio(recordedPath).catch(() => undefined);
      setVoicePath("");
      setVoiceMime("audio/ogg");
    }
  }

  async function startCall() {
    const value = await invoke("Appel démarré", "voice_call", { conversationId });
    if (value && typeof value === "object" && "callId" in value) {
      setActiveCallId(String(value.callId));
    }
  }

  async function endCall() {
    if (!activeCallId) {
      setError("Aucun appel actif connu dans cette fenêtre.");
      return;
    }
    await invoke("Appel terminé", "end_call", { callId: activeCallId });
    setActiveCallId("");
  }

  async function clearVoice() {
    if (voicePath.includes("snapmcp-test-"))
      await core.removeTestAudio(voicePath).catch(() => undefined);
    setVoicePath("");
  }

  async function copySetupCommand(command: string) {
    try {
      await navigator.clipboard.writeText(command);
      setResult({ title: "Commande copiée", value: command });
    } catch {
      setError(
        "Impossible de copier la commande. Sélectionne-la depuis le résultat du diagnostic.",
      );
    }
  }

  const diagnosticOk = (ids: string[] | undefined): boolean =>
    ids !== undefined &&
    ids.length > 0 &&
    ids.every(
      (id) =>
        diagnostics?.checks.some((check) => check.id === id && check.status === "ok") === true,
    );
  const setupDone = (step: SetupStep): boolean =>
    Boolean(setupChecked[step.id]) || diagnosticOk(step.checks);
  const completedSetup = SETUP_STEPS.filter(setupDone).length;
  const nextSetupStep = SETUP_STEPS.find((step) => !setupDone(step));

  return (
    <div className="locaryn-conn-settings">
      <div className="locaryn-box-card" style={{ maxWidth: 760 }}>
        <div className="locaryn-box-head">
          <div>
            <h3 className="locaryn-box-name">Tester SnapMCP</h3>
            <span className="locaryn-box-brand">Une petite fenêtre, des actions directes</span>
          </div>
          <span className={`locaryn-tag${serverReady ? " locaryn-tag-installed" : ""}`}>
            {serverReady ? "prêt" : installed ? "à connecter" : "extension absente"}
          </span>
        </div>{" "}
        <p className="locaryn-box-desc">
          Choisis un backend et utilise les boutons. La connexion MCP et le serveur sont gérés
          automatiquement. Pour Snapchat Web, « Ouvrir… / QR » ouvre Chromium avec le QR code.
          Scanne-le, puis clique sur « Vérifier la connexion Web » : la session est alors
          sauvegardée.
        </p>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor="snapmcp-backend">
              Plateforme
            </label>
            <select
              id="snapmcp-backend"
              className="locaryn-select"
              value={backend}
              onChange={(e) => setBackend(e.target.value as Backend)}
            >
              {BACKENDS.map((item) => (
                <option key={item.value} value={item.value}>
                  {item.label}
                </option>
              ))}
            </select>
            <p className="locaryn-field-hint">{backendInfo.note}</p>
          </div>
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor="snapmcp-conversation">
              Contact ou conversation
            </label>
            <input
              id="snapmcp-conversation"
              className="locaryn-input"
              value={conversationId}
              onChange={(e) => setConversationId(e.target.value)}
              placeholder="@pseudo, ID ou nom Snapchat"
            />
            <p className="locaryn-field-hint">
              Le même identifiant est transmis à la plateforme choisie.
            </p>
          </div>
        </div>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 8 }}>
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={() => void invoke("Connexion vérifiée", "get_conversations")}
            disabled={busy}
          >
            {busy ? "Connexion…" : "Tester la connexion"}
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void openSnapchatWebLogin()}
            disabled={busy}
          >
            Ouvrir Snapchat Web / QR
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void verifySnapchatWebLogin()}
            disabled={busy}
          >
            Vérifier la connexion Web
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void invoke("Conversations", "get_conversations")}
            disabled={busy}
          >
            Lister les conversations
          </button>{" "}
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void refresh()}
            disabled={busy}
          >
            Actualiser
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void runDiagnostics()}
            disabled={busy}
          >
            Diagnostic automatique
          </button>
        </div>
      </div>

      <div className="locaryn-box-card" style={{ maxWidth: 760, marginTop: 12 }}>
        <div className="locaryn-box-head">
          <div>
            <h3 className="locaryn-box-name">Mise en route guidée</h3>
            <span className="locaryn-box-brand">PC, Telegram, Android et Snapchat</span>
          </div>
          <span
            className={`locaryn-tag${completedSetup === SETUP_STEPS.length ? " locaryn-tag-installed" : ""}`}
          >
            {completedSetup}/{SETUP_STEPS.length}
          </span>
        </div>
        <p className="locaryn-box-desc">
          Suis les étapes dans l'ordre. Le diagnostic coche automatiquement les prérequis détectés ;
          tes validations manuelles sont conservées sur ce PC.
        </p>
        <div
          style={{
            height: 6,
            borderRadius: 999,
            background: "var(--surface-muted, rgba(127, 127, 127, 0.14))",
            overflow: "hidden",
            margin: "10px 0 14px",
          }}
        >
          <div
            style={{
              height: "100%",
              width: `${Math.round((completedSetup / SETUP_STEPS.length) * 100)}%`,
              background: "var(--accent, #4f9d69)",
              transition: "width 250ms ease",
            }}
          />
        </div>
        <div style={{ display: "grid", gap: 8 }}>
          {SETUP_STEPS.map((step, index) => {
            const autoDone = diagnosticOk(step.checks);
            const done = setupDone(step);
            const isNext = nextSetupStep?.id === step.id;
            return (
              <div
                key={step.id}
                style={{
                  border: `1px solid ${isNext ? "var(--accent, #4f9d69)" : "var(--border, rgba(127, 127, 127, 0.18))"}`,
                  borderRadius: 8,
                  padding: "9px 10px",
                  background: isNext
                    ? "var(--surface-muted, rgba(127, 127, 127, 0.06))"
                    : undefined,
                }}
              >
                <div style={{ display: "flex", gap: 10, alignItems: "flex-start" }}>
                  <input
                    type="checkbox"
                    checked={done}
                    disabled={autoDone}
                    onChange={() =>
                      setSetupChecked((current) => ({ ...current, [step.id]: !done }))
                    }
                    aria-label={`Étape ${index + 1} : ${step.title}`}
                    style={{ marginTop: 3 }}
                  />
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div
                      style={{ display: "flex", gap: 8, alignItems: "baseline", flexWrap: "wrap" }}
                    >
                      <strong>
                        {index + 1}. {step.title}
                      </strong>
                      <span className="locaryn-field-hint">{step.phase}</span>
                    </div>
                    <div className="locaryn-field-hint">{step.detail}</div>
                    {autoDone && (
                      <div
                        className="locaryn-field-hint"
                        style={{ color: "var(--success, #4f9d69)", marginTop: 3 }}
                      >
                        Détecté automatiquement.
                      </div>
                    )}
                    {step.id === "telegram-session" && !autoDone && (
                      <button
                        type="button"
                        className="locaryn-btn-ghost"
                        onClick={() => void copySetupCommand("npm run telegram:login")}
                        style={{ marginTop: 7 }}
                      >
                        Copier la commande de connexion
                      </button>
                    )}
                    {step.id === "adb-tools" && !autoDone && (
                      <button
                        type="button"
                        className="locaryn-btn-ghost"
                        onClick={() => void copySetupCommand("adb devices")}
                        style={{ marginTop: 7 }}
                      >
                        Copier la commande de vérification
                      </button>
                    )}
                    {step.id === "snapchat-web" && (
                      <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 7 }}>
                        <button
                          type="button"
                          className="locaryn-btn-ghost"
                          onClick={() => void openSnapchatWebLogin()}
                          disabled={busy}
                        >
                          Ouvrir le QR code
                        </button>
                        <button
                          type="button"
                          className="locaryn-btn-ghost"
                          onClick={() => void verifySnapchatWebLogin()}
                          disabled={busy}
                        >
                          Vérifier Snapchat Web
                        </button>
                      </div>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
        {completedSetup === SETUP_STEPS.length && (
          <p
            className="locaryn-field-hint"
            style={{ color: "var(--success, #4f9d69)", marginTop: 12 }}
          >
            Configuration prête. Tu peux passer aux essais réels avec une conversation de test.
          </p>
        )}
      </div>

      <div className="locaryn-box-card" style={{ maxWidth: 760, marginTop: 12 }}>
        <div className="locaryn-box-head">
          <div>
            <h3 className="locaryn-box-name">VM Android légère</h3>
            <span className="locaryn-box-brand">Installation et cycle de vie depuis Locaryn</span>
          </div>
          <span
            className={`locaryn-tag${vmStatus?.runningEmulators.length ? " locaryn-tag-installed" : ""}`}
          >
            {vmStatus?.runningEmulators.length ? "en marche" : "non démarrée"}
          </span>
        </div>
        <p className="locaryn-box-desc">
          Android Emulator officiel, image Google APIs x86_64, 2 Go de RAM, démarrage sans animation
          et caméra/micro configurables. Le bouton prépare l’SDK et l’AVD ; le contrôle du cycle de
          vie passe par l’émulateur, séparé du téléphone ADB physique. L’automatisation de Snapchat
          dans Android utilise toutefois le pont Android de l’émulateur : il n’existe pas de
          contrôle UI universel via la seule console de l’émulateur.
        </p>
        <div className="locaryn-field-hint" style={{ marginBottom: 8 }}>
          {vmStatus?.detail ?? "Clique sur Diagnostiquer pour vérifier le SDK Android."}
        </div>
        {vmStatus && (
          <div className="locaryn-field-hint" style={{ marginBottom: 8 }}>
            SDK : {vmStatus.sdkRoot ?? "introuvable"} · AVD : {vmStatus.avds.join(", ") || "aucune"}
          </div>
        )}
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void refreshVm()}
            disabled={vmBusy}
          >
            Diagnostiquer la VM
          </button>
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={() => void setupVm()}
            disabled={vmBusy}
          >
            {vmBusy ? "Préparation…" : "Installer et configurer la VM"}
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void startVm()}
            disabled={vmBusy || !vmStatus?.avds.length}
          >
            Démarrer la VM
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void stopVm()}
            disabled={vmBusy || !vmStatus?.runningEmulators.length}
          >
            Arrêter la VM
          </button>
        </div>
      </div>

      <div className="locaryn-box-card" style={{ maxWidth: 760, marginTop: 12 }}>
        <div className="locaryn-box-head">
          <div>
            <h3 className="locaryn-box-name">Laboratoire écran Android</h3>
            <span className="locaryn-box-brand">
              Pixels, arbre UI, OCR optionnel et contrôles bornés
            </span>
          </div>
          <span
            className={`locaryn-tag${screenProbe?.bootCompleted ? " locaryn-tag-installed" : ""}`}
          >
            {screenProbe
              ? `${screenProbe.serial} · ${screenProbe.bootCompleted ? "Android prêt" : "boot en cours"}`
              : "non testé"}
          </span>
        </div>
        <p className="locaryn-box-desc">
          Fonctionne sur une AVD vierge ou un téléphone ADB : aucun compte Google, Snapchat ou Play
          Store nécessaire. L’analyse s’appuie d’abord sur l’arbre UI Android ; Tesseract est
          utilisé seulement s’il est installé pour lire les pixels quand un élément n’est pas exposé
          sémantiquement.
        </p>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={() => void probeScreen(true)}
            disabled={screenBusy}
          >
            {screenBusy ? "Analyse…" : "Capturer + analyser l’écran"}
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void probeScreen(false)}
            disabled={screenBusy}
          >
            UI XML sans OCR
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void screenAction("back")}
            disabled={screenBusy || !screenProbe}
          >
            Retour Android
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void screenAction("home")}
            disabled={screenBusy || !screenProbe}
          >
            Accueil Android
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void screenAction("refresh")}
            disabled={screenBusy || !screenProbe}
          >
            Actualiser
          </button>
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(4, minmax(70px, 1fr))",
            gap: 8,
            marginTop: 10,
          }}
        >
          <input
            className="locaryn-input"
            inputMode="numeric"
            value={screenX}
            onChange={(e) => setScreenX(e.target.value)}
            aria-label="X"
            placeholder="X"
          />
          <input
            className="locaryn-input"
            inputMode="numeric"
            value={screenY}
            onChange={(e) => setScreenY(e.target.value)}
            aria-label="Y"
            placeholder="Y"
          />
          <input
            className="locaryn-input"
            inputMode="numeric"
            value={screenX2}
            onChange={(e) => setScreenX2(e.target.value)}
            aria-label="X2"
            placeholder="X2"
          />
          <input
            className="locaryn-input"
            inputMode="numeric"
            value={screenY2}
            onChange={(e) => setScreenY2(e.target.value)}
            aria-label="Y2"
            placeholder="Y2"
          />
        </div>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 8 }}>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void screenAction("tap")}
            disabled={screenBusy || !screenProbe}
          >
            Tapper X/Y
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() => void screenAction("swipe")}
            disabled={screenBusy || !screenProbe}
          >
            Glisser X/Y → X2/Y2
          </button>
        </div>
        {screenProbe && (
          <div style={{ display: "grid", gap: 10, marginTop: 12 }}>
            {screenProbe.screenshotBase64 && (
              <img
                src={`data:image/png;base64,${screenProbe.screenshotBase64}`}
                alt={`Capture écran ${screenProbe.serial}`}
                style={{
                  maxWidth: "100%",
                  maxHeight: 420,
                  objectFit: "contain",
                  background: "#090d10",
                  borderRadius: 8,
                }}
              />
            )}
            <div className="locaryn-field-hint">
              Résolution : {screenProbe.displaySize ?? "inconnue"} · OCR :{" "}
              {screenProbe.ocrAvailable ? "disponible" : "non installé"}
              <br />
              {screenProbe.ocrDetail}
            </div>
            {screenProbe.ocrText && (
              <pre style={{ whiteSpace: "pre-wrap", maxHeight: 180, overflow: "auto" }}>
                {screenProbe.ocrText}
              </pre>
            )}
            <details>
              <summary>Textes UI détectés ({screenProbe.uiText.length})</summary>
              <pre style={{ whiteSpace: "pre-wrap", maxHeight: 180, overflow: "auto" }}>
                {screenProbe.uiText.join("\\n") || "Aucun texte sémantique."}
              </pre>
            </details>
            <details>
              <summary>Arbre UI XML brut</summary>
              <pre style={{ whiteSpace: "pre-wrap", maxHeight: 220, overflow: "auto" }}>
                {screenProbe.uiXml}
              </pre>
            </details>
          </div>
        )}
      </div>

      {diagnostics && (
        <div className="locaryn-box-card" style={{ maxWidth: 760, marginTop: 12 }}>
          <div className="locaryn-box-head">
            <div>
              <h3 className="locaryn-box-name">Diagnostic</h3>
              <span className="locaryn-box-brand">Vérification sans modifier de fichier</span>
            </div>
            <span className="locaryn-tag">
              {diagnostics.checks.filter((item) => item.status === "ok").length}/
              {diagnostics.checks.length} OK
            </span>
          </div>
          <div style={{ display: "grid", gap: 8, marginTop: 12 }}>
            {diagnostics.checks.map((item) => {
              const color =
                item.status === "ok"
                  ? "var(--success, #4f9d69)"
                  : item.status === "warning"
                    ? "var(--warning, #b57b28)"
                    : "var(--danger)";
              return (
                <div
                  key={item.id}
                  style={{
                    borderLeft: `3px solid ${color}`,
                    padding: "7px 10px",
                    background: "var(--surface-muted, rgba(127, 127, 127, 0.08))",
                  }}
                >
                  <strong style={{ color }}>{item.label}</strong>
                  <div className="locaryn-field-hint">{item.detail}</div>
                  {item.value && (
                    <code style={{ display: "block", wordBreak: "break-all", marginTop: 4 }}>
                      {item.value}
                    </code>
                  )}
                  {item.fix && (
                    <div className="locaryn-field-hint" style={{ marginTop: 4 }}>
                      Action : {item.fix}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
          gap: 12,
          marginTop: 12,
        }}
      >
        <div className="locaryn-box-card">
          <h4 className="locaryn-box-name">Message texte</h4>
          <textarea
            className="locaryn-input"
            rows={3}
            value={text}
            onChange={(e) => setText(e.target.value)}
            style={{ resize: "vertical", fontFamily: "inherit" }}
          />
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={() => void invoke("Message envoyé", "send_message", { conversationId, text })}
            disabled={busy || !text.trim()}
          >
            Envoyer le texte
          </button>
        </div>

        <div className="locaryn-box-card">
          <h4 className="locaryn-box-name">Image ou vidéo</h4>
          <input
            className="locaryn-input"
            value={mediaUrl}
            onChange={(e) => setMediaUrl(e.target.value)}
            placeholder="URL ou chemin local"
          />
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8, marginTop: 8 }}>
            <select
              className="locaryn-select"
              value={mediaType}
              onChange={(e) => setMediaType(e.target.value as "image" | "video")}
              aria-label="Type de média"
            >
              <option value="image">Image</option>
              <option value="video">Vidéo</option>
            </select>
            <select
              className="locaryn-select"
              value={visibility}
              onChange={(e) => setVisibility(e.target.value as MediaVisibility)}
              aria-label="Visibilité du média"
            >
              <option value="saved">Conservée</option>
              <option value="timed_10s" disabled={backend !== "telegram"}>
                10 secondes
              </option>
              <option value="view_once" disabled={backend !== "telegram"}>
                Vue unique
              </option>
              <option value="view_once_replay" disabled={backend !== "telegram"}>
                Relecture unique non supportée
              </option>
            </select>
          </div>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 8 }}>
            <button
              type="button"
              className="locaryn-btn-ghost"
              onClick={() => void chooseMedia()}
              disabled={busy}
            >
              Choisir un fichier
            </button>
            <button
              type="button"
              className="locaryn-btn-primary"
              onClick={() => void sendMedia()}
              disabled={busy || !mediaUrl.trim()}
            >
              Envoyer le média
            </button>
          </div>
          <p className="locaryn-field-hint">Telegram éphémère : photo privée uniquement.</p>
        </div>

        <div className="locaryn-box-card">
          <h4 className="locaryn-box-name">Vocal</h4>
          <p className="locaryn-field-hint">
            Le navigateur enregistre le micro du PC. ADB utilise le micro du téléphone.
          </p>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button
              type="button"
              className="locaryn-btn-ghost"
              onClick={recording ? stopRecording : startRecording}
              disabled={busy}
            >
              {recording ? "Arrêter" : "Enregistrer au micro"}
            </button>
            <button
              type="button"
              className="locaryn-btn-ghost"
              onClick={() => void chooseAudio()}
              disabled={busy || recording}
            >
              Choisir un audio
            </button>
            {voicePath && (
              <button
                type="button"
                className="locaryn-btn-ghost"
                onClick={() => void clearVoice()}
                disabled={busy}
              >
                Effacer
              </button>
            )}
          </div>
          {voicePath && (
            <code
              className="locaryn-connector-cmd"
              style={{ display: "block", marginTop: 8, wordBreak: "break-all" }}
            >
              {voicePath} · {voiceMime}
            </code>
          )}
          <button
            type="button"
            className="locaryn-btn-primary"
            onClick={() => void sendVoice()}
            disabled={busy || (!voicePath && !text.trim())}
            style={{ marginTop: 10 }}
          >
            Envoyer le vocal
          </button>
        </div>

        <div className="locaryn-box-card">
          <h4 className="locaryn-box-name">Appel vocal</h4>
          <p className="locaryn-field-hint">Disponible selon le backend et sa session connectée.</p>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button
              type="button"
              className="locaryn-btn-primary"
              onClick={() => void startCall()}
              disabled={busy}
            >
              Démarrer l'appel
            </button>
            <button
              type="button"
              className="locaryn-btn-ghost"
              onClick={() => void endCall()}
              disabled={busy || !activeCallId}
            >
              Raccrocher
            </button>
          </div>
          {activeCallId && <p className="locaryn-field-hint">Appel actif : {activeCallId}</p>}
        </div>
      </div>

      {error && (
        <div className="locaryn-box-card" style={{ marginTop: 12, borderColor: "var(--danger)" }}>
          <strong>Erreur</strong>
          <p className="locaryn-field-hint" style={{ color: "var(--danger)" }}>
            {error}
          </p>
        </div>
      )}
      {result && (
        <div className="locaryn-box-card" style={{ marginTop: 12 }}>
          <strong>{result.title}</strong>
          <pre
            style={{ whiteSpace: "pre-wrap", overflowX: "auto", margin: "10px 0 0", fontSize: 12 }}
          >
            {pretty(result.value)}
          </pre>
        </div>
      )}
    </div>
  );
}
