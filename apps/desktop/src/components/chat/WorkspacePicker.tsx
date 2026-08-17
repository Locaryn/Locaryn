import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { FREE_CHAT_PATH } from "../../lib/constants";
import { type Project, type SshServer, core } from "../../lib/core";

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
  /** Whether the app is currently connected to a Locaryn cloud server.
   *  When false, the Cloud workspace is shown as disabled/off. */
  cloudConnected?: boolean;
};

const KIND_META: Record<WorkspaceKind, { icon: IconName; label: string; hint: string }> = {
  cloud: {
    icon: "cloud",
    label: "Cloud",
    hint: "Aucun accès fichier — conversation seule",
  },
  local: { icon: "project", label: "Local", hint: "Un dossier de votre machine" },
  ssh: { icon: "server", label: "SSH", hint: "Un serveur distant enregistré" },
  temp: {
    icon: "archive",
    label: "Temporaire",
    hint: "Dossier temporaire créé automatiquement pour cette conversation",
  },
};

/**
 * Where the agent works: nothing (cloud), a local folder, or a saved SSH
 * server. Sits above the composer because it changes what every message can
 * touch — that context should be visible without opening settings.
 */
export function WorkspacePicker({
  value,
  onChange,
  onAddProject,
  onAddSsh,
  freeChat,
  cloudConnected,
}: Props) {
  const [open, setOpen] = useState(false);
  const [projects, setProjects] = useState<Project[]>([]);
  const [servers, setServers] = useState<SshServer[]>([]);
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    core
      .listProjects()
      .then((list) => setProjects(list.filter((p) => p.path !== FREE_CHAT_PATH)))
      .catch(() => {});
    core
      .listSshServers()
      .then(setServers)
      .catch(() => setServers([]));
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
      <div className="locaryn-ws">
        <button
          type="button"
          className="locaryn-ws-trigger"
          disabled
          title={meta.hint}
          style={{ cursor: "default" }}
        >
          <span style={{ display: "inline-flex" }}>
            <Icon name={meta.icon} size={15} />
          </span>
          <span className="locaryn-ws-label">{meta.label}</span>
        </button>
      </div>
    );
  }

  return (
    <div className="locaryn-ws" ref={ref}>
      <button
        type="button"
        className="locaryn-ws-trigger"
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="menu"
        aria-expanded={open}
        title={meta.hint}
      >
        <span style={{ display: "inline-flex" }}>
          <Icon name={meta.icon} size={15} />
        </span>
        <span className="locaryn-ws-label">{value.label}</span>
        <span className="locaryn-ws-caret">{open ? "▾" : "▸"}</span>
      </button>

      {open && (
        <div className="locaryn-ws-menu" role="menu">
          <button
            type="button"
            className={`locaryn-ws-item${value.kind === "cloud" && cloudConnected ? " locaryn-active" : ""}`}
            disabled={!cloudConnected}
            onClick={() => {
              if (!cloudConnected) return;
              onChange({ kind: "cloud", id: null, label: "Cloud" });
              setOpen(false);
            }}
            title={
              cloudConnected
                ? KIND_META.cloud.hint
                : "Cloud indisponible — connectez l'app à un serveur Locaryn"
            }
            style={!cloudConnected ? { opacity: 0.5, cursor: "not-allowed" } : undefined}
          >
            <span style={{ display: "inline-flex" }}>
              <Icon name="cloud" size={15} />
            </span>
            <span className="locaryn-ws-item-text">
              <span>Cloud {cloudConnected ? "" : "(off)"}</span>
              <span className="locaryn-ws-item-hint">
                {cloudConnected
                  ? KIND_META.cloud.hint
                  : "Connectez l'app à un serveur Locaryn pour activer"}
              </span>
            </span>
          </button>

          <div className="locaryn-ws-sep">
            <span>
              <Icon name="project" size={14} /> Dossiers locaux
            </span>
            {onAddProject && (
              <button
                type="button"
                className="locaryn-ws-add"
                onClick={() => {
                  setOpen(false);
                  onAddProject();
                }}
                title="Ajouter un dossier"
              >
                +
              </button>
            )}
          </div>
          {projects.length === 0 ? (
            <div className="locaryn-ws-empty">Aucun projet</div>
          ) : (
            projects.map((p) => (
              <button
                key={p.id}
                type="button"
                className={`locaryn-ws-item${value.kind === "local" && value.id === p.id ? " locaryn-active" : ""}`}
                onClick={() => {
                  onChange({ kind: "local", id: p.id, label: p.name });
                  setOpen(false);
                }}
                title={p.path}
              >
                <span style={{ display: "inline-flex" }}>
                  <Icon name="project" size={15} />
                </span>
                <span className="locaryn-ws-item-text">
                  <span>{p.name}</span>
                  <span className="locaryn-ws-item-hint">{p.path}</span>
                </span>
              </button>
            ))
          )}

          <div className="locaryn-ws-sep">
            <span>
              <Icon name="server" size={14} /> Serveurs SSH
            </span>
            {onAddSsh && (
              <button
                type="button"
                className="locaryn-ws-add"
                onClick={() => {
                  setOpen(false);
                  onAddSsh();
                }}
                title="Ajouter une connexion SSH"
              >
                +
              </button>
            )}
          </div>
          {servers.length === 0 ? (
            <div className="locaryn-ws-empty">Aucun serveur enregistré</div>
          ) : (
            servers.map((s) => (
              <button
                key={s.id}
                type="button"
                className={`locaryn-ws-item${value.kind === "ssh" && value.id === s.id ? " locaryn-active" : ""}`}
                onClick={() => {
                  onChange({ kind: "ssh", id: s.id, label: s.name });
                  setOpen(false);
                }}
                title={`${s.username}@${s.host}:${s.port}`}
              >
                <span style={{ display: "inline-flex" }}>
                  <Icon name="server" size={15} />
                </span>
                <span className="locaryn-ws-item-text">
                  <span>{s.name}</span>
                  <span className="locaryn-ws-item-hint">
                    {s.username}@{s.host}
                  </span>
                </span>
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
}
