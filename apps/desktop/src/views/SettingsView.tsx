import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { AboutSettings } from "../components/AboutSettings";
import { CautionSettings } from "../components/CautionSettings";
import { ConnectionSettings } from "../components/ConnectionSettings";
import { ConnectorsSettings } from "../components/ConnectorsSettings";
import { EngineSettings } from "../components/EngineSettings";
import { ExtensionsSettings } from "../components/ExtensionsSettings";
import { HuggingFaceSettings } from "../components/HuggingFaceSettings";
import { PairingCodes } from "../components/PairingCodes";
import { PerformancePanel } from "../components/PerformancePanel";
import { ServerSettings } from "../components/ServerSettings";
import { StorageSettings } from "../components/StorageSettings";
import { TravelSettings } from "../components/TravelSettings";
import type { UseThemeReturn } from "../hooks/useTheme";
import { ACCENT_PRESETS } from "../hooks/useTheme";
import { type AppInfo, type Project, type Session, core } from "../lib/core";
import { getPendingInstall, subscribeDeepLink } from "../lib/deepLink";
import { DO_NOT_TRANSLATE, LANGUAGES, useI18n } from "../lib/i18n";
import { ProjectSettings } from "./ProjectSettings";

export type Section =
  | "engine"
  | "performance"
  | "conversation"
  | "huggingface"
  | "projects"
  | "extensions"
  | "connectors"
  | "appearance"
  | "language"
  | "server"
  | "storage"
  | "about";

type Props = {
  theme: UseThemeReturn;
  projects: Project[];
  sessionsByProject: Record<string, Session[]>;
  standaloneSessions: Session[];
  /** Capacités des extensions actives ; Remote déverrouille le tunnel. */
  activeCapabilities?: string[];
  initialSection?: Section;
  onOpenSession?: (session: Session) => void;
  onProjectArchived?: (p: Project) => void;
  /** Jump to the model marketplace (models are managed there, not here). */
  onOpenMarketplace?: () => void;
};

const SECTIONS: { id: Section; icon: IconName; label: string; desc: string }[] = [
  {
    id: "engine",
    icon: "settings",
    label: "Moteur IA",
    desc: "Runtime llama.cpp, capacités, adaptateurs LoRA",
  },
  {
    id: "performance",
    icon: "speed",
    label: "Performance",
    desc: "GPU, cache KV, contexte, offload",
  },
  {
    id: "conversation",
    icon: "chat",
    label: "Conversation",
    desc: "Historique, titres et conversations récentes",
  },
  {
    id: "huggingface",
    icon: "marketplace",
    label: "HuggingFace",
    desc: "Token pour les dépôts restreints (modèles gated)",
  },
  {
    id: "projects",
    icon: "project",
    label: "Projets",
    desc: "Autorisations, base de connaissances, archivage",
  },
  {
    id: "extensions",
    icon: "extensions",
    label: "Extensions",
    desc: "Extensions Locaryn, plugins compatibles et noyaux",
  },
  {
    id: "connectors",
    icon: "server",
    label: "Connecteurs & MCP",
    desc: "Connexions SSH, bases de données et serveurs MCP",
  },
  { id: "appearance", icon: "studio", label: "Apparence", desc: "Couleur d'accentuation, thème" },
  { id: "language", icon: "chat", label: "Langue", desc: "Langue de l'interface" },
  {
    id: "server",
    icon: "server",
    label: "Serveur & fonctions",
    desc: "Service Locaryn, accès local et appairage",
  },
  {
    id: "storage",
    icon: "models",
    label: "Stockage",
    desc: "Emplacement des modèles, espace disque, nettoyage",
  },
  { id: "about", icon: "warning", label: "À propos", desc: "Version, licences, système" },
];

/**
 * Full-page general settings, reached from the left navigation. Distinct from
 * the compact chat settings popup: this covers the whole application (engine,
 * projects, extensions, appearance, storage) while the popup only carries the
 * chat-scoped knobs. Shared sections are the same components in both.
 */
export function SettingsView({
  theme,
  projects,
  sessionsByProject,
  standaloneSessions,
  activeCapabilities = [],
  initialSection,
  onOpenSession,
  onProjectArchived,
  onOpenMarketplace,
}: Props) {
  const { settings, updateAccent, resetTheme } = theme;
  const { lang, setLang } = useI18n();
  const [section, setSection] = useState<Section>(initialSection ?? "engine");
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    if (initialSection) setSection(initialSection);
  }, [initialSection]);

  useEffect(() => {
    let cancelled = false;
    core
      .appInfo()
      .then((i) => {
        if (!cancelled) setInfo(i);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // Un lien locaryn://install?src=… doit atterrir sur la section Extensions.
  // Le panneau consomme ensuite l'intention pour pré-remplir la fenêtre
  // d'installation — on ne fait que naviguer ici.
  useEffect(() => {
    const check = () => {
      if (getPendingInstall()) setSection("extensions");
    };
    check();
    return subscribeDeepLink(check);
  }, []);

  const current = SECTIONS.find((s) => s.id === section)!;
  const remoteEnabled = activeCapabilities.includes("travel-tunnel");

  return (
    <section className="locaryn-view-container locaryn-settings-page">
      <div className="locaryn-view-header">
        <h2>Paramètres Système</h2>
        <p className="locaryn-view-desc">
          Tous les réglages de Locaryn. Les options propres à une conversation restent accessibles
          depuis le panneau du chat.
        </p>
      </div>

      <div className="locaryn-settings-full">
        <nav className="locaryn-settings-full-nav">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              type="button"
              className={`locaryn-settings-full-item${section === s.id ? " locaryn-active" : ""}`}
              onClick={() => setSection(s.id)}
            >
              <span className="locaryn-settings-full-icon">
                <Icon name={s.icon} />
              </span>
              <span className="locaryn-settings-full-text">
                <span className="locaryn-settings-full-label">{s.label}</span>
                <span className="locaryn-settings-full-desc">{s.desc}</span>
              </span>
            </button>
          ))}
        </nav>

        <div className="locaryn-settings-full-pane">
          <h3 className="locaryn-settings-full-title">
            <Icon name={current.icon} size={18} /> {current.label}
          </h3>

          {section === "engine" && <EngineSettings activeCapabilities={activeCapabilities} />}
          {section === "performance" && (
            <>
              <PerformancePanel />
              <CautionSettings />
            </>
          )}
          {section === "conversation" && (
            <ConversationHistorySettings
              projects={projects}
              sessionsByProject={sessionsByProject}
              standaloneSessions={standaloneSessions}
              onOpenSession={onOpenSession}
            />
          )}
          {section === "huggingface" && <HuggingFaceSettings />}
          {section === "projects" && (
            <ProjectSettings projects={projects} onArchived={onProjectArchived} />
          )}
          {section === "extensions" && <ExtensionsSettings />}
          {section === "connectors" && <ConnectorsSettings />}

          {section === "appearance" && (
            <div className="locaryn-field">
              <div className="locaryn-field-label">Couleur d'accentuation</div>
              <p className="locaryn-field-hint">
                La teinte unique de l'interface. Sobre et naturelle par défaut.
              </p>
              <div className="locaryn-swatch-grid" style={{ marginTop: 12 }}>
                {ACCENT_PRESETS.map((p) => (
                  <button
                    key={p.hex}
                    type="button"
                    className={`locaryn-swatch${settings.accentHex === p.hex ? " locaryn-swatch-active" : ""}`}
                    style={{ background: p.hex }}
                    title={p.name}
                    aria-label={`Accent ${p.name}`}
                    onClick={() => updateAccent(p.hex)}
                  >
                    {settings.accentHex === p.hex && (
                      <span className="locaryn-swatch-check">
                        <Icon name="check" size={14} />
                      </span>
                    )}
                  </button>
                ))}
              </div>
              <div className="locaryn-custom-color" style={{ marginTop: 16 }}>
                <input
                  type="color"
                  value={settings.accentHex}
                  onChange={(e) => updateAccent(e.target.value)}
                  className="locaryn-color-input"
                  aria-label="Couleur personnalisée"
                />
                <span className="locaryn-color-value">{settings.accentHex}</span>
              </div>
              <button
                type="button"
                className="locaryn-settings-reset"
                style={{ marginTop: 16 }}
                onClick={resetTheme}
              >
                Réinitialiser l'apparence
              </button>
            </div>
          )}

          {section === "server" && (
            <div className="locaryn-network-layout">
              <div className="locaryn-network-config">
                <ServerSettings />
                {remoteEnabled && <TravelSettings />}
                <ConnectionSettings />
              </div>
              <PairingCodes remoteEnabled={remoteEnabled} />
            </div>
          )}

          {section === "storage" && <StorageSettings onOpenMarketplace={onOpenMarketplace} />}

          {section === "language" && (
            <div className="locaryn-field">
              <div className="locaryn-field-label">Langue de l'interface</div>
              <p className="locaryn-field-hint">
                Change la langue des textes de l'application. Les noms de modèles, de marques et les
                termes techniques restent inchangés.
              </p>
              <div className="locaryn-lang-grid" style={{ marginTop: 12 }}>
                {LANGUAGES.map((l) => (
                  <button
                    key={l.id}
                    type="button"
                    className={`locaryn-lang-item${lang === l.id ? " locaryn-active" : ""}`}
                    onClick={() => setLang(l.id)}
                  >
                    <span className="locaryn-lang-flag">{l.flag}</span>
                    <span>{l.label}</span>
                    {lang === l.id && (
                      <span className="locaryn-lang-check">
                        <Icon name="check" size={14} />
                      </span>
                    )}
                  </button>
                ))}
              </div>
              <p className="locaryn-field-hint" style={{ marginTop: 16 }}>
                <strong>Jamais traduits</strong> — noms de produits et termes techniques :
              </p>
              <div className="locaryn-lang-terms">
                {DO_NOT_TRANSLATE.slice(0, 14).map((w) => (
                  <code key={w}>{w}</code>
                ))}
                <span className="locaryn-field-hint">+{DO_NOT_TRANSLATE.length - 14} autres</span>
              </div>
            </div>
          )}

          {section === "about" && <AboutSettings />}
        </div>
      </div>
    </section>
  );
}

function ConversationHistorySettings({
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
        if (!search.trim()) return true;
        const q = search.toLowerCase();
        return (s.title ?? "").toLowerCase().includes(q) || project.name.toLowerCase().includes(q);
      }),
    }))
    .filter((group) => group.sessions.length > 0);

  const filteredStandalone = standaloneSessions.filter((s) => {
    if (!search.trim()) return true;
    const q = search.toLowerCase();
    return (s.title ?? "").toLowerCase().includes(q) || "libre non groupé".includes(q);
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
              label="Conversations libres (non groupées)"
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
