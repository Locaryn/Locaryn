import { useEffect, useRef, useState } from "react";
import { core, type Project, type SshServer } from "../../lib/core";
import { FREE_CHAT_PATH } from "../../lib/constants";

export type WorkspaceKind = "cloud" | "local" | "ssh" | "temp";

export interface WorkspaceSelection {
  kind: WorkspaceKind;
  /** Project (local) or SSH server id; null for cloud/temp. */
  id: string | null;
  label: string;
}

type Props = {
  value: WorkspaceSelection;
  onChange: (next: WorkspaceSelection) => void;
  /** Add a local project by path (same flow as the sidebar). */
  onAddProject?: () => void;
  /** Open the SSH connector form to register a new server. */
  onAddSsh?: () => void;
  /** True when the current chat is a free conversation (no project).
   *  Free chats use an auto-created temp folder, so local/SSH options are hidden. */
  freeChat?: boolean;
  /** Whether the app is currently connected to a Lochor cloud server.
   *  When false, the Cloud workspace is shown as disabled/off. */
  cloudConnected?: boolean;
};

const KIND_META: Record<WorkspaceKind, { icon: string; label: string; hint: string }> = {
  cloud: { icon: "☁️", label: "Cloud", hint: "Aucun accès fichier — conversation seule" },
  local: { icon: "📁", label: "Local", hint: "Un dossier de votre machine" },
  ssh: { icon: "🖧", label: "SSH", hint: "Un serveur distant enregistré" },
  temp: { icon: "🗂️", label: "Temporaire", hint: "Dossier temporaire créé automatiquement pour cette conversation" },
};

/**
 * Where the agent works: nothing (cloud), a local folder, or a saved SSH
 * server. Sits above the composer because it changes what every message can
 * touch — that context should be visible without opening settings.
 */
export function WorkspacePicker({ value, onChange, onAddProject, onAddSsh, freeChat, cloudConnected }: Props) {
  const [open, setOpen] = useState(false);
  const [projects, setProjects] = useState<Project[]>([]);
  const [servers, setServers] = useState<SshServer[]>([]);
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    core.listProjects()
      .then((list) => setProjects(list.filter((p) => p.path !== FREE_CHAT_PATH)))
      .catch(() => {});
    core.listSshServers().then(setServers).catch(() => setServers([]));
    function onDown(e: MouseEvent) {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    }
    function onEsc(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onEsc);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onEsc);
    };
  }, [open]);

  const meta = KIND_META[value.kind];

  // Free conversations get an auto-created temp folder. The user doesn't need
  // to pick or even see a path, so the picker is reduced to a read-only label.
  if (freeChat) {
    const meta = value.kind === "temp" ? KIND_META[value.kind] : KIND_META.temp;
    return (
      <div className="lochor-ws">
        <button
          type="button"
          className="lochor-ws-trigger"
          disabled
          title={meta.hint}
          style={{ cursor: "default" }}
        >
          <span>{meta.icon}</span>
          <span className="lochor-ws-label">{meta.label}</span>
        </button>
      </div>
    );
  }

  return (
    <div className="lochor-ws" ref={ref}>
      <button
        type="button"
        className="lochor-ws-trigger"
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="menu"
        aria-expanded={open}
        title={meta.hint}
      >
        <span>{meta.icon}</span>
        <span className="lochor-ws-label">{value.label}</span>
        <span className="lochor-ws-caret">{open ? "▾" : "▸"}</span>
      </button>

      {open && (
        <div className="lochor-ws-menu" role="menu">
          <button
            type="button"
            className={`lochor-ws-item${value.kind === "cloud" && cloudConnected ? " lochor-active" : ""}`}
            disabled={!cloudConnected}
            onClick={() => {
              if (!cloudConnected) return;
              onChange({ kind: "cloud", id: null, label: "Cloud" });
              setOpen(false);
            }}
            title={cloudConnected ? KIND_META.cloud.hint : "Cloud indisponible — connectez l'app à un serveur Lochor"}
            style={!cloudConnected ? { opacity: 0.5, cursor: "not-allowed" } : undefined}
          >
            <span>☁️</span>
            <span className="lochor-ws-item-text">
              <span>Cloud {cloudConnected ? "" : "(off)"}</span>
              <span className="lochor-ws-item-hint">
                {cloudConnected ? KIND_META.cloud.hint : "Connectez l'app à un serveur Lochor pour activer"}
              </span>
            </span>
          </button>

          <div className="lochor-ws-sep">
            <span>📁 Dossiers locaux</span>
            {onAddProject && (
              <button type="button" className="lochor-ws-add" onClick={() => { setOpen(false); onAddProject(); }} title="Ajouter un dossier">
                +
              </button>
            )}
          </div>
          {projects.length === 0 ? (
            <div className="lochor-ws-empty">Aucun projet</div>
          ) : (
            projects.map((p) => (
              <button
                key={p.id}
                type="button"
                className={`lochor-ws-item${value.kind === "local" && value.id === p.id ? " lochor-active" : ""}`}
                onClick={() => {
                  onChange({ kind: "local", id: p.id, label: p.name });
                  setOpen(false);
                }}
                title={p.path}
              >
                <span>📁</span>
                <span className="lochor-ws-item-text">
                  <span>{p.name}</span>
                  <span className="lochor-ws-item-hint">{p.path}</span>
                </span>
              </button>
            ))
          )}

          <div className="lochor-ws-sep">
            <span>🖧 Serveurs SSH</span>
            {onAddSsh && (
              <button type="button" className="lochor-ws-add" onClick={() => { setOpen(false); onAddSsh(); }} title="Ajouter une connexion SSH">
                +
              </button>
            )}
          </div>
          {servers.length === 0 ? (
            <div className="lochor-ws-empty">Aucun serveur enregistré</div>
          ) : (
            servers.map((s) => (
              <button
                key={s.id}
                type="button"
                className={`lochor-ws-item${value.kind === "ssh" && value.id === s.id ? " lochor-active" : ""}`}
                onClick={() => {
                  onChange({ kind: "ssh", id: s.id, label: s.name });
                  setOpen(false);
                }}
                title={`${s.username}@${s.host}:${s.port}`}
              >
                <span>🖧</span>
                <span className="lochor-ws-item-text">
                  <span>{s.name}</span>
                  <span className="lochor-ws-item-hint">{s.username}@{s.host}</span>
                </span>
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
}
