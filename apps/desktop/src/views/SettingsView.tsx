import { Icon, type IconName, type ThemeMode } from "@locaryn/ui-core";
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
import { type AccountSection, AccountView } from "./AccountView";
import { ProjectSettings } from "./ProjectSettings";

/**
 * Les sous-sections du compte.
 *
 * Elles vivent dans le rail, pas dans un second menu à côté du premier : deux
 * menus et un contenu tenaient trois colonnes, et le contenu qu'on cherchait
 * finissait dans le tiers restant.
 */
const ACCOUNT_SECTIONS: { id: AccountSection; label: string; desc: string; icon: IconName }[] = [
  { id: "profile", label: "Profil local", desc: "Identité, avatar, connexion", icon: "private" },
  {
    id: "models",
    label: "Préférences des modèles",
    desc: "Petites tâches, voix et images",
    icon: "models",
  },
  {
    id: "conversations",
    label: "Conversations",
    desc: "Historique et conversations récentes",
    icon: "chat",
  },
  { id: "memory", label: "Mémoire", desc: "Ce que Locaryn retient", icon: "memory" },
  { id: "archives", label: "Archives", desc: "Conversations rangées", icon: "archive" },
];

/** Les trois réglages de thème, dans l'ordre où ils se lisent. */
const THEME_MODES: { value: ThemeMode; label: string; icon: IconName }[] = [
  { value: "dark", label: "Sombre", icon: "moon" },
  { value: "light", label: "Clair", icon: "sun" },
  { value: "system", label: "Système", icon: "monitor" },
];

export type Section =
  | "account"
  | "extensions"
  | "connectors"
  | "projects"
  | "engine"
  | "performance"
  | "huggingface"
  | "server"
  | "storage"
  | "appearance"
  | "language"
  | "about";

type SettingsCategory = "user" | "ai" | "server" | "system";

type SectionDef = {
  id: Section;
  icon: IconName;
  label: string;
  desc: string;
  category: SettingsCategory;
};

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

const SECTIONS: SectionDef[] = [
  // ── Espace Utilisateur & Outils ──
  {
    id: "account",
    icon: "private",
    label: "Compte & Profil",
    desc: "Profil local, identité, préférences et mémoire",
    category: "user",
  },
  {
    id: "extensions",
    icon: "extensions",
    label: "Morphs & Skills",
    desc: "Morphs Locaryn (UI & moteurs), compétences et packs d'agents",
    category: "user",
  },
  {
    id: "connectors",
    icon: "server",
    label: "Connecteurs & MCP",
    desc: "Serveurs MCP, bases de données et passerelles techniques",
    category: "user",
  },
  {
    id: "projects",
    icon: "project",
    label: "Projets & Permissions",
    desc: "Autorisations d'outils, base de connaissances, archivage",
    category: "user",
  },

  // ── Intelligence Artificielle & Noyau ──
  {
    id: "engine",
    icon: "settings",
    label: "Moteur IA & Noyau",
    desc: "Runtime llama.cpp, configuration du noyau, offload et adaptateurs",
    category: "ai",
  },
  {
    id: "performance",
    icon: "speed",
    label: "Performance & GPU",
    desc: "GPU, cache KV, contexte, offload et benchmarks",
    category: "ai",
  },
  {
    id: "huggingface",
    icon: "marketplace",
    label: "HuggingFace",
    desc: "Token pour les dépôts restreints (modèles gated)",
    category: "ai",
  },

  // ── Serveur & Infrastructure ──
  {
    id: "server",
    icon: "server",
    label: "Serveur & Tunnels",
    desc: "Service Locaryn, accès local, réseau et appairage",
    category: "server",
  },

  // ── Système & Préférences ──
  {
    id: "storage",
    icon: "models",
    label: "Stockage & Modèles",
    desc: "Emplacement des modèles, espace disque, nettoyage",
    category: "system",
  },
  {
    id: "appearance",
    icon: "studio",
    label: "Apparence & Thème",
    desc: "Couleur d'accentuation, mode sombre / clair",
    category: "system",
  },
  {
    id: "language",
    icon: "chat",
    label: "Langue",
    desc: "Langue de l'interface utilisateur",
    category: "system",
  },
  {
    id: "about",
    icon: "warning",
    label: "À propos",
    desc: "Version, licences et diagnostic système",
    category: "system",
  },
];

const CATEGORY_HEADERS: Record<SettingsCategory, string> = {
  user: "MON ESPACE & EXTENSIBILITÉ",
  ai: "INTELLIGENCE ARTIFICIELLE & NOYAU",
  server: "SERVEUR & ÉQUIPE",
  system: "SYSTÈME & PRÉFÉRENCES",
};

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
  const { settings, updateAccent, updateMode, resetTheme } = theme;
  const { lang, setLang } = useI18n();
  const [section, setSection] = useState<Section>(initialSection ?? "account");
  // Le rail descend d'un cran quand une section a ses propres sous-sections.
  // Le retour remonte sans changer ce qui est affiché à droite : on revient
  // choisir autre chose, on ne perd pas ce qu'on regardait.
  const [railLevel, setRailLevel] = useState<"root" | "account">("root");
  const [accountSection, setAccountSection] = useState<AccountSection>("profile");
  // Fenêtre étroite : le rail et le volet ne tiennent pas côte à côte, alors
  // ils se relaient. Au large, les deux colonnes restent visibles et cet état
  // ne change rien — d'où le pilotage par un attribut, laissé au CSS.
  const [paneOpen, setPaneOpen] = useState(false);
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

  useEffect(() => {
    const check = () => {
      if (getPendingInstall()) setSection("extensions");
    };
    check();
    return subscribeDeepLink(check);
  }, []);

  const current = SECTIONS.find((s) => s.id === section) || SECTIONS[0];
  // Dans le compte, le volet porte le nom de la sous-section : le titre doit
  // dire ce qu'on regarde, pas la famille dont ça vient.
  const currentAccount = ACCOUNT_SECTIONS.find((a) => a.id === accountSection);
  const currentTitle =
    section === "account" && currentAccount
      ? { icon: currentAccount.icon, label: currentAccount.label }
      : { icon: current.icon, label: current.label };
  const remoteEnabled = activeCapabilities.includes("travel-tunnel");

  const categoriesOrder: SettingsCategory[] = ["user", "ai", "server", "system"];

  return (
    <section className="locaryn-view-container locaryn-settings-page">
      <div className="locaryn-view-header">
        <h2>Paramètres Système &amp; Profil</h2>
        <p className="locaryn-view-desc">
          Configuration générale de votre profil, de vos Morphs, de vos compétences et du moteur
          d'inférence.
        </p>
      </div>

      <div className="locaryn-settings-full" data-pane={paneOpen ? "open" : "closed"}>
        <nav className="locaryn-settings-full-nav">
          {railLevel === "account" ? (
            <>
              <button
                type="button"
                className="locaryn-settings-back"
                onClick={() => setRailLevel("root")}
              >
                <Icon name="back" size={16} />
                Tous les réglages
              </button>
              <span className="locaryn-settings-group-title">Compte &amp; Profil</span>
              <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                {ACCOUNT_SECTIONS.map((a) => (
                  <button
                    key={a.id}
                    type="button"
                    className={`locaryn-settings-full-item${accountSection === a.id ? " locaryn-active" : ""}`}
                    onClick={() => {
                      setAccountSection(a.id);
                      setPaneOpen(true);
                    }}
                  >
                    <span className="locaryn-settings-full-icon">
                      <Icon name={a.icon} />
                    </span>
                    <span className="locaryn-settings-full-text">
                      <span className="locaryn-settings-full-label">{a.label}</span>
                      <span className="locaryn-settings-full-desc">{a.desc}</span>
                    </span>
                  </button>
                ))}
              </div>
            </>
          ) : (
            categoriesOrder.map((cat) => {
              const catSections = SECTIONS.filter((s) => s.category === cat);
              if (catSections.length === 0) return null;

              return (
                <div key={cat} style={{ marginBottom: 12 }}>
                  <span className="locaryn-settings-group-title">{CATEGORY_HEADERS[cat]}</span>
                  <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                    {catSections.map((s) => (
                      <button
                        key={s.id}
                        type="button"
                        className={`locaryn-settings-full-item${section === s.id ? " locaryn-active" : ""}`}
                        onClick={() => {
                          setSection(s.id);
                          setPaneOpen(true);
                          if (s.id === "account") setRailLevel("account");
                        }}
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
                  </div>
                </div>
              );
            })
          )}
        </nav>

        <div className="locaryn-settings-full-pane">
          <button
            type="button"
            className="locaryn-settings-pane-back"
            onClick={() => setPaneOpen(false)}
          >
            <Icon name="back" size={16} />
            {railLevel === "account" ? "Compte & Profil" : "Réglages"}
          </button>
          <h3 className="locaryn-settings-full-title">
            <Icon name={currentTitle.icon} size={18} /> {currentTitle.label}
          </h3>

          {section === "account" && (
            <AccountView
              embedded
              section={accountSection}
              onSectionChange={setAccountSection}
              activeCapabilities={activeCapabilities}
              projects={projects}
              sessionsByProject={sessionsByProject}
              standaloneSessions={standaloneSessions}
              onOpenSession={(session) => onOpenSession?.(session)}
            />
          )}
          {section === "engine" && <EngineSettings activeCapabilities={activeCapabilities} />}
          {section === "performance" && (
            <>
              <PerformancePanel />
              <CautionSettings />
            </>
          )}
          {section === "huggingface" && <HuggingFaceSettings />}
          {section === "projects" && (
            <ProjectSettings projects={projects} onArchived={onProjectArchived} />
          )}
          {section === "extensions" && <ExtensionsSettings />}
          {section === "connectors" && <ConnectorsSettings />}

          {section === "appearance" && (
            <div className="locaryn-field">
              <div className="locaryn-field-label">Thème</div>
              <p className="locaryn-field-hint">
                Sombre par défaut. En clair, l'accent s'assombrit tout seul pour rester lisible.
              </p>
              <div className="locaryn-segmented" style={{ marginTop: 12 }} role="group">
                {THEME_MODES.map((m) => (
                  <button
                    key={m.value}
                    type="button"
                    className={`locaryn-segment${settings.mode === m.value ? " locaryn-segment-on" : ""}`}
                    aria-pressed={settings.mode === m.value}
                    onClick={() => updateMode(m.value)}
                  >
                    <Icon name={m.icon} size={14} />
                    {m.label}
                  </button>
                ))}
              </div>

              <div className="locaryn-field-label" style={{ marginTop: 32 }}>
                Couleur d'accentuation
              </div>
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
