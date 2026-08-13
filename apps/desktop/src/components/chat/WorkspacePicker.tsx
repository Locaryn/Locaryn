import { useEffect, useRef, useState } from "react";
import { FREE_CHAT_PATH } from "../../lib/constants";
import { type Project, type SshServer, core } from "../../lib/core";

export type WorkspaceKind = "local" | "ssh" | "temp";

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
};

const KIND_META: Record<WorkspaceKind, { icon: string; label: string; hint: string }> = {
  local: { icon: "📁", label: "Local", hint: "Un dossier de votre machine" },
  ssh: { icon: "🖧", label: "SSH", hint: "Un serveur distant enregistré" },
  temp: {
    icon: "🗂️",
    label: "Espace de travail",
    hint: "Dossier de travail créé automatiquement dès que du code ou des fichiers sont utilisés",
  },
};

/**
 * Where the agent works: a local folder or a saved SSH server.
 * server. Sits above the composer because it changes what every message can
 * touch — that context should be visible without opening settings.
 */
export function WorkspacePicker({ value, onChange, onAddProject, onAddSsh, freeChat }: Props) {
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

  // Free conversations get an auto-created temp folder — but only once code
  // or files actually need a directory. Until then the label is empty and the
  // picker is hidden entirely: a plain question must not clutter the composer
  // with a useless "Temporary" badge (and must not create a folder on disk).
  if (freeChat) {
    if (!value.label) return null;
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
          <span>{meta.icon}</span>
          <span className="locaryn-ws-label">{value.label}</span>
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
        <span>{meta.icon}</span>
        <span className="locaryn-ws-label">{value.label}</span>
        <span className="locaryn-ws-caret">{open ? "▾" : "▸"}</span>
      </button>

      {open && (
        <div className="locaryn-ws-menu" role="menu">
          <div className="locaryn-ws-sep">
            <span>📁 Dossiers locaux</span>
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
                <span>📁</span>
                <span className="locaryn-ws-item-text">
                  <span>{p.name}</span>
                  <span className="locaryn-ws-item-hint">{p.path}</span>
                </span>
              </button>
            ))
          )}

          <div className="locaryn-ws-sep">
            <span>🖧 Serveurs SSH</span>
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
                <span>🖧</span>
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
