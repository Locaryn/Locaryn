import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { FREE_CHAT_PATH } from "../../lib/constants";
import { type InstalledExtension, type Project, core } from "../../lib/core";
import { getSlotContributions } from "../extensions/SlotRegistry";

export type WorkspaceKind = "local" | "ssh" | "remote" | "none" | "cloud" | "temp";

export interface WorkspaceSelection {
  kind: WorkspaceKind;
  /** Project (local) or SSH server id; null for none/cloud/temp. */
  id: string | null;
  label: string;
  path?: string | null;
  extensionId?: string | null;
}

export interface ExtensionWorkspaceTarget {
  id: string;
  name: string;
  address: string;
  extensionId: string;
}

const STORAGE_SSH_WORKSPACES = "locaryn_ssh_workspaces";

function loadSavedSshWorkspaces(): ExtensionWorkspaceTarget[] {
  try {
    const raw = localStorage.getItem(STORAGE_SSH_WORKSPACES);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveSshWorkspaces(list: ExtensionWorkspaceTarget[]): void {
  try {
    localStorage.setItem(STORAGE_SSH_WORKSPACES, JSON.stringify(list));
  } catch {
    // ignore
  }
}

type Props = {
  value: WorkspaceSelection;
  onChange: (next: WorkspaceSelection) => void;
  /** Add a local project by path (same flow as the sidebar). */
  onAddProject?: () => void;
  /** Whether the app is currently connected to a Locaryn remote server or cloud. */
  cloudConnected?: boolean;
  remoteServerName?: string | null;
  extensions?: InstalledExtension[];
};

/**
 * Where the agent works: a local folder, an extension-provided SSH server,
 * a remote environment, or standalone.
 * Sits above the composer so the active working environment is always visible and configurable.
 */
export function WorkspacePicker({
  value,
  onChange,
  onAddProject,
  cloudConnected,
  remoteServerName,
  extensions = [],
}: Props) {
  const [open, setOpen] = useState(false);
  const [projects, setProjects] = useState<Project[]>([]);
  const [sshWorkspaces, setSshWorkspaces] = useState<ExtensionWorkspaceTarget[]>(loadSavedSshWorkspaces);
  const [addSshModalOpen, setAddSshModalOpen] = useState(false);
  const [sshName, setSshName] = useState("");
  const [sshHost, setSshHost] = useState("");
  const [sshUser, setSshUser] = useState("root");
  const [sshPath, setSshPath] = useState("/root");
  const ref = useRef<HTMLDivElement | null>(null);

  // Check if an extension provides SSH / remote workspace capabilities
  const workspaceSlots = getSlotContributions(extensions, "chat.workspaces");
  const hasSshExtension =
    workspaceSlots.length > 0 ||
    extensions.some(
      (e) =>
        e.enabled &&
        (e.capabilities?.includes("ssh-remote-exec") || e.capabilities?.includes("remote-workspace")),
    );

  useEffect(() => {
    if (!open) return;
    core
      .listProjects()
      .then((list) => setProjects(list.filter((p) => p.path !== FREE_CHAT_PATH)))
      .catch(() => {});
    setSshWorkspaces(loadSavedSshWorkspaces());

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

  function handleSaveNewSsh() {
    const name = sshName.trim() || sshHost.trim() || "Serveur SSH";
    const host = sshHost.trim();
    const user = sshUser.trim() || "root";
    const targetPath = sshPath.trim() || "~";
    if (!host) return;

    const address = `${user}@${host}:${targetPath}`;
    const newTarget: ExtensionWorkspaceTarget = {
      id: `ssh_${Date.now()}`,
      name,
      address,
      extensionId: "extension-ssh",
    };

    const updated = [...sshWorkspaces, newTarget];
    setSshWorkspaces(updated);
    saveSshWorkspaces(updated);

    onChange({
      kind: "ssh",
      id: newTarget.id,
      label: newTarget.name,
      path: newTarget.address,
      extensionId: newTarget.extensionId,
    });

    setAddSshModalOpen(false);
    setSshName("");
    setSshHost("");
    setSshUser("root");
    setSshPath("/root");
    setOpen(false);
  }

  function handleRemoveSsh(e: React.MouseEvent, id: string) {
    e.stopPropagation();
    const updated = sshWorkspaces.filter((s) => s.id !== id);
    setSshWorkspaces(updated);
    saveSshWorkspaces(updated);
    if (value.kind === "ssh" && value.id === id) {
      onChange({ kind: "none", id: null, label: "Dossier de travail", path: null });
    }
  }

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
  const displayLabel = value.label || "Dossier de travail";
  const displayTitle = value.path ?? value.label;

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
          borderColor: isCustomSelected ? "var(--border-strong)" : "var(--border)",
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

          {/* ── Emplacements Distants (Apportés dynamiquement par plugin-ssh) ── */}
          {hasSshExtension && (
            <>
              <div className="locaryn-ws-sep">
                <span>
                  <Icon name="server" size={14} /> Emplacements SSH (extension-ssh)
                </span>
                <button
                  type="button"
                  className="locaryn-ws-add"
                  onClick={() => setAddSshModalOpen(true)}
                  title="Ajouter un emplacement SSH"
                >
                  +
                </button>
              </div>

              <button
                type="button"
                className="locaryn-ws-item"
                onClick={() => setAddSshModalOpen(true)}
                style={{ color: "var(--accent)" }}
              >
                <span style={{ display: "inline-flex" }}>
                  <Icon name="server" size={15} />
                </span>
                <span className="locaryn-ws-item-text">
                  <span>+ Ajouter un emplacement SSH...</span>
                  <span className="locaryn-ws-item-hint">
                    Définir un serveur et dossier distant pour le chat
                  </span>
                </span>
              </button>

              {sshWorkspaces.map((s) => (
                <div
                  key={s.id}
                  style={{ display: "flex", alignItems: "center", width: "100%" }}
                  className={`locaryn-ws-item${value.kind === "ssh" && value.id === s.id ? " locaryn-active" : ""}`}
                  onClick={() => {
                    onChange({
                      kind: "ssh",
                      id: s.id,
                      label: s.name,
                      path: s.address,
                      extensionId: s.extensionId,
                    });
                    setOpen(false);
                  }}
                  title={s.address}
                >
                  <span style={{ display: "inline-flex" }}>
                    <Icon name="server" size={15} />
                  </span>
                  <span className="locaryn-ws-item-text" style={{ flex: 1 }}>
                    <span>{s.name}</span>
                    <span className="locaryn-ws-item-hint">{s.address}</span>
                  </span>
                  <button
                    type="button"
                    className="locaryn-icon-btn"
                    style={{ padding: "4px", fontSize: "11px", opacity: 0.6 }}
                    onClick={(e) => handleRemoveSsh(e, s.id)}
                    title="Retirer cet emplacement"
                  >
                    ×
                  </button>
                </div>
              ))}
            </>
          )}

          {/* ── Serveur distant / Mode client ── */}
          {cloudConnected && (
            <>
              <div className="locaryn-ws-sep">
                <span>
                  <Icon name="server" size={14} /> Serveur distant (Mode Client)
                </span>
              </div>

              <button
                type="button"
                className={`locaryn-ws-item${value.kind === "remote" ? " locaryn-active" : ""}`}
                onClick={() => {
                  onChange({
                    kind: "remote",
                    id: "remote",
                    label: remoteServerName ? `Serveur (${remoteServerName})` : "Serveur distant",
                    path: "remote://server",
                  });
                  setOpen(false);
                }}
                title="Exécuter et travailler dans l'environnement du serveur distant"
              >
                <span style={{ display: "inline-flex" }}>
                  <Icon name="server" size={15} />
                </span>
                <span className="locaryn-ws-item-text">
                  <span>
                    {remoteServerName ? `Serveur ${remoteServerName}` : "Serveur distant"} (Connecté)
                  </span>
                  <span className="locaryn-ws-item-hint">
                    Environnement de travail du serveur distant
                  </span>
                </span>
              </button>
            </>
          )}

          {/* ── Réinitialiser (Conversation libre) ── */}
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

      {/* Modal d'ajout rapide d'emplacement SSH */}
      {addSshModalOpen && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.65)",
            zIndex: 9999,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: "16px",
          }}
          onClick={() => setAddSshModalOpen(false)}
        >
          <div
            className="locaryn-card"
            style={{ width: "100%", maxWidth: "440px", padding: "20px" }}
            onClick={(e) => e.stopPropagation()}
          >
            <h3 style={{ margin: "0 0 16px 0", fontSize: "16px" }}>
              Ajouter un emplacement de travail SSH
            </h3>
            <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
              <div>
                <label className="locaryn-field-label">Nom / Alias</label>
                <input
                  className="locaryn-input"
                  placeholder="ex: Serveur Prod"
                  value={sshName}
                  onChange={(e) => setSshName(e.target.value)}
                />
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 2fr", gap: "8px" }}>
                <div>
                  <label className="locaryn-field-label">Utilisateur</label>
                  <input
                    className="locaryn-input"
                    placeholder="ubuntu"
                    value={sshUser}
                    onChange={(e) => setSshUser(e.target.value)}
                  />
                </div>
                <div>
                  <label className="locaryn-field-label">Hôte / IP</label>
                  <input
                    className="locaryn-input"
                    placeholder="192.168.1.100 ou srv.domaine.com"
                    value={sshHost}
                    onChange={(e) => setSshHost(e.target.value)}
                  />
                </div>
              </div>
              <div>
                <label className="locaryn-field-label">Répertoire distant cible</label>
                <input
                  className="locaryn-input"
                  placeholder="/var/www/mon-projet ou /root"
                  value={sshPath}
                  onChange={(e) => setSshPath(e.target.value)}
                />
              </div>
            </div>
            <div style={{ display: "flex", justifyContent: "flex-end", gap: "8px", marginTop: "20px" }}>
              <button
                type="button"
                className="locaryn-btn-ghost"
                onClick={() => setAddSshModalOpen(false)}
              >
                Annuler
              </button>
              <button
                type="button"
                className="locaryn-btn-primary"
                disabled={!sshHost.trim()}
                onClick={handleSaveNewSsh}
              >
                Sélectionner cet emplacement
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
