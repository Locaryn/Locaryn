import { useCallback, useEffect, useRef, useState } from "react";
import {
  type InstalledExtension,
  type McpServerInfo,
  type SnapMcpDiagnostics,
  core,
} from "../lib/core";
import { pickAnyFile } from "../lib/dialog";

type Backend = "mock" | "web" | "adb" | "telegram";
type MediaVisibility = "saved" | "timed_10s" | "view_once" | "view_once_replay";

type RunResult = {
  title: string;
  value: unknown;
};

const BACKENDS: { value: Backend; label: string; note: string }[] = [
  { value: "mock", label: "Simulation", note: "Aucun compte et aucun envoi réel." },
  { value: "telegram", label: "Telegram", note: "Compte personnel via MTProto." },
  { value: "web", label: "Snapchat Web", note: "Session Playwright sur web.snapchat.com." },
  { value: "adb", label: "Snapchat Android", note: "Téléphone connecté avec ADB." },
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
      const selected = nextServers.find((item) => /snapmcp|snap-astreinte/i.test(item.name))
        ?? nextServers[0];
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
      const mime = ["audio/ogg;codecs=opus", "audio/webm;codecs=opus", "audio/webm"].find(
        (candidate) => MediaRecorder.isTypeSupported(candidate),
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
          setResult({ title: "Enregistrement terminé", value: { path, mimeType: blob.type, bytes: blob.size } });
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
    const selected = available.find((item) => /snapmcp|snap-astreinte/i.test(item.name))
      ?? available[0];
    if (!selected) throw new Error("Serveur SnapMCP introuvable. Recharge l'extension.");
    if (!selected.running) await core.startMcpServer(selected.name);
    setServers(available.map((item) => item.name === selected.name ? { ...item, running: true } : item));
    setServerName(selected.name);
    setConfiguredBackend(selectedBackend);
    return selected.name;
  }

  async function invoke(title: string, tool: string, args: Record<string, unknown> = {}, selectedBackend = backend) {
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
    await invoke("Connexion Snapchat Web vérifiée", "web_session_status", {}, "web");
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
    await invoke("Vocal envoyé", "send_voice_note", {
      conversationId,
      audioPath: voicePath || undefined,
      text: text.trim() || undefined,
      language: "fr-FR",
    });
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
    if (voicePath.includes("snapmcp-test-")) await core.removeTestAudio(voicePath).catch(() => undefined);
    setVoicePath("");
  }

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
        </div>          <p className="locaryn-box-desc">
            Choisis un backend et utilise les boutons. La connexion MCP et le serveur sont gérés automatiquement.
            Pour Snapchat Web, « Ouvrir… / QR » ouvre Chromium avec le QR code. Scanne-le, puis clique sur « Vérifier la connexion Web » : la session est alors sauvegardée.
          </p>

        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor="snapmcp-backend">Plateforme</label>
            <select id="snapmcp-backend" className="locaryn-select" value={backend} onChange={(e) => setBackend(e.target.value as Backend)}>
              {BACKENDS.map((item) => <option key={item.value} value={item.value}>{item.label}</option>)}
            </select>
            <p className="locaryn-field-hint">{backendInfo.note}</p>
          </div>
          <div className="locaryn-field">
            <label className="locaryn-field-label" htmlFor="snapmcp-conversation">Contact ou conversation</label>
            <input id="snapmcp-conversation" className="locaryn-input" value={conversationId} onChange={(e) => setConversationId(e.target.value)} placeholder="@pseudo, ID ou nom Snapchat" />
            <p className="locaryn-field-hint">Le même identifiant est transmis à la plateforme choisie.</p>
          </div>
        </div>

        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 8 }}>          <button
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
          <button type="button" className="locaryn-btn-ghost" onClick={() => void invoke("Conversations", "get_conversations")} disabled={busy}>
            Lister les conversations
          </button>          <button
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

      {diagnostics && (
        <div className="locaryn-box-card" style={{ maxWidth: 760, marginTop: 12 }}>
          <div className="locaryn-box-head">
            <div>
              <h3 className="locaryn-box-name">Diagnostic</h3>
              <span className="locaryn-box-brand">Vérification sans modifier de fichier</span>
            </div>
            <span className="locaryn-tag">
              {diagnostics.checks.filter((item) => item.status === "ok").length}/{diagnostics.checks.length} OK
            </span>
          </div>
          <div style={{ display: "grid", gap: 8, marginTop: 12 }}>
            {diagnostics.checks.map((item) => {
              const color = item.status === "ok"
                ? "var(--success, #4f9d69)"
                : item.status === "warning"
                  ? "var(--warning, #b57b28)"
                  : "var(--danger)";
              return (
                <div key={item.id} style={{ borderLeft: `3px solid ${color}`, padding: "7px 10px", background: "var(--surface-muted, rgba(127, 127, 127, 0.08))" }}>
                  <strong style={{ color }}>{item.label}</strong>
                  <div className="locaryn-field-hint">{item.detail}</div>
                  {item.value && <code style={{ display: "block", wordBreak: "break-all", marginTop: 4 }}>{item.value}</code>}
                  {item.fix && <div className="locaryn-field-hint" style={{ marginTop: 4 }}>Action : {item.fix}</div>}
                </div>
              );
            })}
          </div>
        </div>
      )}

      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))", gap: 12, marginTop: 12 }}>
        <div className="locaryn-box-card">
          <h4 className="locaryn-box-name">Message texte</h4>
          <textarea className="locaryn-input" rows={3} value={text} onChange={(e) => setText(e.target.value)} style={{ resize: "vertical", fontFamily: "inherit" }} />
          <button type="button" className="locaryn-btn-primary" onClick={() => void invoke("Message envoyé", "send_message", { conversationId, text })} disabled={busy || !text.trim()}>
            Envoyer le texte
          </button>
        </div>

        <div className="locaryn-box-card">
          <h4 className="locaryn-box-name">Image ou vidéo</h4>
          <input className="locaryn-input" value={mediaUrl} onChange={(e) => setMediaUrl(e.target.value)} placeholder="URL ou chemin local" />
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8, marginTop: 8 }}>
            <select className="locaryn-select" value={mediaType} onChange={(e) => setMediaType(e.target.value as "image" | "video")} aria-label="Type de média">
              <option value="image">Image</option>
              <option value="video">Vidéo</option>
            </select>
            <select className="locaryn-select" value={visibility} onChange={(e) => setVisibility(e.target.value as MediaVisibility)} aria-label="Visibilité du média">
              <option value="saved">Conservée</option>
              <option value="timed_10s" disabled={backend !== "telegram"}>10 secondes</option>
              <option value="view_once" disabled={backend !== "telegram"}>Vue unique</option>
              <option value="view_once_replay" disabled={backend !== "telegram"}>Relecture unique non supportée</option>
            </select>
          </div>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 8 }}>
            <button type="button" className="locaryn-btn-ghost" onClick={() => void chooseMedia()} disabled={busy}>Choisir un fichier</button>
            <button type="button" className="locaryn-btn-primary" onClick={() => void sendMedia()} disabled={busy || !mediaUrl.trim()}>Envoyer le média</button>
          </div>
          <p className="locaryn-field-hint">Telegram éphémère : photo privée uniquement.</p>
        </div>

        <div className="locaryn-box-card">
          <h4 className="locaryn-box-name">Vocal</h4>
          <p className="locaryn-field-hint">Le navigateur enregistre le micro du PC. ADB utilise le micro du téléphone.</p>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button type="button" className="locaryn-btn-ghost" onClick={recording ? stopRecording : startRecording} disabled={busy}>
              {recording ? "Arrêter" : "Enregistrer au micro"}
            </button>
            <button type="button" className="locaryn-btn-ghost" onClick={() => void chooseAudio()} disabled={busy || recording}>Choisir un audio</button>
            {voicePath && <button type="button" className="locaryn-btn-ghost" onClick={() => void clearVoice()} disabled={busy}>Effacer</button>}
          </div>
          {voicePath && <code className="locaryn-connector-cmd" style={{ display: "block", marginTop: 8, wordBreak: "break-all" }}>{voicePath} · {voiceMime}</code>}
          <button type="button" className="locaryn-btn-primary" onClick={() => void sendVoice()} disabled={busy || (!voicePath && !text.trim())} style={{ marginTop: 10 }}>
            Envoyer le vocal
          </button>
        </div>

        <div className="locaryn-box-card">
          <h4 className="locaryn-box-name">Appel vocal</h4>
          <p className="locaryn-field-hint">Disponible selon le backend et sa session connectée.</p>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button type="button" className="locaryn-btn-primary" onClick={() => void startCall()} disabled={busy}>Démarrer l'appel</button>
            <button type="button" className="locaryn-btn-ghost" onClick={() => void endCall()} disabled={busy || !activeCallId}>Raccrocher</button>
          </div>
          {activeCallId && <p className="locaryn-field-hint">Appel actif : {activeCallId}</p>}
        </div>
      </div>

      {error && <div className="locaryn-box-card" style={{ marginTop: 12, borderColor: "var(--danger)" }}><strong>Erreur</strong><p className="locaryn-field-hint" style={{ color: "var(--danger)" }}>{error}</p></div>}
      {result && <div className="locaryn-box-card" style={{ marginTop: 12 }}><strong>{result.title}</strong><pre style={{ whiteSpace: "pre-wrap", overflowX: "auto", margin: "10px 0 0", fontSize: 12 }}>{pretty(result.value)}</pre></div>}
    </div>
  );
}
