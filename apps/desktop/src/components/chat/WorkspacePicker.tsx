import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { FREE_CHAT_PATH } from "../../lib/constants";
import { type Project, type SshServer, core } from "../../lib/core";

export type WorkspaceKind = "local" | "ssh" | "remote" | "none" | "cloud" | "temp";

export interface WorkspaceSelection {
  kind: WorkspaceKind;
  /** Project (local) or SSH server id; null for none/cloud/temp. */
  id: string | null;
  label: string;
  path?: string | null;
}

type Props = {
  value: WorkspaceSelection;
  onChange: (next: WorkspaceSelection) => void;
  /** Add a local project by path (same flow as the sidebar). */
  onAddProject?: () => void;
  /** Open the SSH connector form to register a new server. */
  onAddSsh?: () => void;
  /** Whether the app is currently connected to a Locaryn remote server or cloud. */
  cloudConnected?: boolean;
  remoteServerName?: string | null;
};

/**
 * Where the agent works: a local folder, an SSH server, a remote environment, or standalone.
 * Sits above the composer so the active working environment is always visible and configurable.
 */
export function WorkspacePicker({
  value,
  onChange,
  onAddProject,
  onAddSsh,
  cloudConnected,
  remoteServerName,
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

  const isCustomSelected =
    (value.kind === "local" || value.kind === "ssh" || value.kind === "remote") &&
    Boolean(
      value.id ||
        (value.label &&
          value.label !== "Temporaire" &&
          value.label !== "Dossier de travail" &&
          value.label !== "None"),
    );

  const displayIcon: IconName =
    value.kind === "ssh" ? "server" : value.kind === "remote" ? "server" : "project";
  const displayLabel = isCustomSelected ? value.label : "Dossier de travail";
  const displayTitle = isCustomSelected
    ? (value.path ?? value.label)
    : "Sélectionner un dossier local, une connexion SSH ou un environnement distant";

  return (
    <div className="locaryn-ws" ref={ref}>
      <button
        type="button"
        className="locaryn-ws-trigger"
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="menu"
        aria-expanded={open}
        title={displayTitle}
        style={{
          borderColor: isCustomSelected ? "var(--border-strong)" : undefined,
          color: isCustomSelected ? "var(--text)" : "var(--text-dim)",
        }}
      >
        <span style={{ display: "inline-flex" }}>
          <Icon name={displayIcon} size={14} />
        </span>
        <span className="locaryn-ws-label">{displayLabel}</span>
        <span className="locaryn-ws-caret">{open ? "▾" : "▸"}</span>
      </button>

      {open && (
        <div className="locaryn-ws-menu" role="menu">
          {/* ── Dossiers locaux ── */}
          <div className="locaryn-ws-sep" style={{ marginTop: 0, borderTop: "none" }}>
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
                title="Ouvrir un dossier local"
              >
                +
              </button>
            )}
          </div>

          {onAddProject && (
            <button
              type="button"
              className="locaryn-ws-item"
              onClick={() => {
                setOpen(false);
                onAddProject();
              }}
              style={{ color: "var(--accent)" }}
            >
              <span style={{ display: "inline-flex" }}>
                <Icon name="project" size={15} />
              </span>
              <span className="locaryn-ws-item-text">
                <span>+ Ouvrir un dossier local...</span>
                <span className="locaryn-ws-item-hint">Choisir un dossier sur cette machine</span>
              </span>
            </button>
          )}

          {projects.map((p) => (
            <button
              key={p.id}
              type="button"
              className={`locaryn-ws-item${value.kind === "local" && value.id === p.id ? " locaryn-active" : ""}`}
              onClick={() => {
                onChange({ kind: "local", id: p.id, label: p.name, path: p.path });
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
          ))}

          {/* ── Serveurs SSH ── */}
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

          {onAddSsh && (
            <button
              type="button"
              className="locaryn-ws-item"
              onClick={() => {
                setOpen(false);
                onAddSsh();
              }}
              style={{ color: "var(--accent)" }}
            >
              <span style={{ display: "inline-flex" }}>
                <Icon name="server" size={15} />
              </span>
              <span className="locaryn-ws-item-text">
                <span>+ Ajouter une connexion SSH...</span>
                <span className="locaryn-ws-item-hint">
                  Configurer un nouvel accès distant par SSH
                </span>
              </span>
            </button>
          )}

          {servers.map((s) => (
            <button
              key={s.id}
              type="button"
              className={`locaryn-ws-item${value.kind === "ssh" && value.id === s.id ? " locaryn-active" : ""}`}
              onClick={() => {
                onChange({
                  kind: "ssh",
                  id: s.id,
                  label: s.name,
                  path: `${s.username}@${s.host}:${s.port}`,
                });
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
                  {s.username}@{s.host}:{s.port}
                </span>
              </span>
            </button>
          ))}

          {/* ── Serveur distant / Mode client ── */}
          <div className="locaryn-ws-sep">
            <span>
              <Icon name="server" size={14} /> Serveur distant (Mode Client)
            </span>
          </div>

          <button
            type="button"
            className={`locaryn-ws-item${value.kind === "remote" && cloudConnected ? " locaryn-active" : ""}`}
            disabled={!cloudConnected}
            onClick={() => {
              if (!cloudConnected) return;
              onChange({
                kind: "remote",
                id: "remote",
                label: remoteServerName ? `Serveur (${remoteServerName})` : "Serveur distant",
                path: "remote://server",
              });
              setOpen(false);
            }}
            title={
              cloudConnected
                ? "Exécuter et travailler dans l'environnement du serveur distant"
                : "Connectez l'application à un serveur Locaryn pour activer l'environnement distant"
            }
            style={!cloudConnected ? { opacity: 0.5, cursor: "not-allowed" } : undefined}
          >
            <span style={{ display: "inline-flex" }}>
              <Icon name="server" size={15} />
            </span>
            <span className="locaryn-ws-item-text">
              <span>
                {remoteServerName ? `Serveur ${remoteServerName}` : "Serveur distant"}{" "}
                {cloudConnected ? "(Connecté)" : "(Déconnecté)"}
              </span>
              <span className="locaryn-ws-item-hint">
                {cloudConnected
                  ? "Environnement de travail du serveur distant"
                  : "Connectez l'application à un serveur distant pour activer"}
              </span>
            </span>
          </button>

          {/* ── Réinitialiser (Conversation sans dossier) ── */}
          {isCustomSelected && (
            <>
              <div className="locaryn-ws-sep">
                <span>
                  <Icon name="close" size={14} /> Réinitialiser
                </span>
              </div>
              <button
                type="button"
                className="locaryn-ws-item"
                onClick={() => {
                  onChange({ kind: "none", id: null, label: "Dossier de travail", path: null });
                  setOpen(false);
                }}
              >
                <span style={{ display: "inline-flex" }}>
                  <Icon name="close" size={15} />
                </span>
                <span className="locaryn-ws-item-text">
                  <span>Aucun dossier spécifique</span>
                  <span className="locaryn-ws-item-hint">
                    Conversation libre sans dossier assigné
                  </span>
                </span>
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}
