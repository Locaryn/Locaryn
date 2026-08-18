import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import type { Project, Session } from "../lib/core";

/**
 * L'historique des conversations, consultable et réouvert depuis les réglages.
 *
 * Les conversations récentes sont une affaire de compte, pas une catégorie de
 * réglages : ce panneau vit dans Compte, rangé avec le profil, la mémoire et
 * les archives.
 */
export function ConversationHistorySettings({
  projects,
  sessionsByProject,
  standaloneSessions,
  onOpenSession,
}: {
  projects: Project[];
  sessionsByProject: Record<string, Session[]>;
  standaloneSessions: Session[];
  onOpenSession?: (session: Session) => void;
}) {
  const [search, setSearch] = useState("");

  const projectGroups = projects
    .map((project) => ({
      project,
      sessions: (sessionsByProject[project.id] ?? []).filter((s) => {
        if (s.ephemeral) return false;
        if (!search.trim()) return true;
        const q = search.toLowerCase();
        return (s.title ?? "").toLowerCase().includes(q) || project.name.toLowerCase().includes(q);
      }),
    }))
    .filter((group) => group.sessions.length > 0);

  const filteredStandalone = standaloneSessions.filter((s) => {
    if (s.ephemeral) return false;
    if (!search.trim()) return true;
    const q = search.toLowerCase();
    return (s.title ?? "").toLowerCase().includes(q);
  });

  const total =
    filteredStandalone.length +
    projectGroups.reduce((sum, group) => sum + group.sessions.length, 0);

  const rawTotal =
    standaloneSessions.length +
    Object.values(sessionsByProject).reduce((sum, list) => sum + list.length, 0);

  return (
    <div className="locaryn-conversation-settings">
      <div className="locaryn-conversation-intro">
        <div>
          <h4>Historique des conversations</h4>
          <p>
            Retrouvez et reprenez l'ensemble de vos échanges récents par espace de travail. Cliquez
            sur une conversation pour l'ouvrir directement dans le Chat. Les conversations archivées
            sont rangées dans <strong>Compte → Archives</strong>.
          </p>
        </div>
        <span className="locaryn-conversation-count">
          {total} conversation{total > 1 ? "s" : ""}
        </span>
      </div>

      {rawTotal > 0 && (
        <div style={{ marginBottom: "16px" }}>
          <input
            className="locaryn-input"
            style={{ width: "100%", fontSize: "13px" }}
            placeholder="Rechercher une conversation par titre ou projet…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
      )}

      {total === 0 ? (
        <div className="locaryn-conversation-empty">
          {search
            ? "Aucune conversation ne correspond à votre recherche."
            : "Aucune conversation récente à afficher."}
        </div>
      ) : (
        <div className="locaryn-conversation-groups">
          {filteredStandalone.length > 0 && (
            <ConversationHistoryGroup
              label="Conversations"
              sessions={filteredStandalone}
              onOpenSession={onOpenSession}
            />
          )}
          {projectGroups.map(({ project, sessions }) => (
            <ConversationHistoryGroup
              key={project.id}
              label={project.name}
              sessions={sessions}
              onOpenSession={onOpenSession}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function ConversationHistoryGroup({
  label,
  sessions,
  onOpenSession,
}: {
  label: string;
  sessions: Session[];
  onOpenSession?: (session: Session) => void;
}) {
  return (
    <section className="locaryn-conversation-group">
      <h5>{label}</h5>
      <div className="locaryn-conversation-list">
        {sessions.map((session) => (
          <button
            key={session.id}
            type="button"
            className="locaryn-conversation-row"
            disabled={!onOpenSession}
            onClick={() => onOpenSession?.(session)}
          >
            <span className="locaryn-conversation-dot" aria-hidden="true" />
            <span className="locaryn-conversation-row-text">
              <strong>{session.title || "Nouvelle conversation"}</strong>
              <small>
                {session.last_message_at
                  ? dateCourte(session.last_message_at)
                  : session.created_at
                    ? dateCourte(session.created_at)
                    : "Conversation locale"}
              </small>
            </span>
            <Icon name="forward" size={14} />
          </button>
        ))}
      </div>
    </section>
  );
}

function dateCourte(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}
