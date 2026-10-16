import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useMemo, useState } from "react";
import { AboutSettings } from "../components/AboutSettings";
import { ApiKeysSettings } from "../components/ApiKeysSettings";
import { CautionSettings } from "../components/CautionSettings";
import { ConnectionSettings } from "../components/ConnectionSettings";
import { ConnectorsSettings } from "../components/ConnectorsSettings";
import { DecisionSettings } from "../components/DecisionSettings";
import { DeviceCompanionCard } from "../components/DeviceCompanionCard";
import { EngineSettings } from "../components/EngineSettings";
import { ExtensionsSettings } from "../components/ExtensionsSettings";
import { GitSettings } from "../components/GitSettings";
import { HuggingFaceSettings } from "../components/HuggingFaceSettings";
import { ModeleAutrePc } from "../components/ModeleAutrePc";
import { NpuSettings } from "../components/NpuSettings";
import { PairingCodes } from "../components/PairingCodes";
import { PerformancePanel } from "../components/PerformancePanel";
import {
  API_FOURNISSEUR_EVENT,
  OUVRIR_CLES_API_EVENT,
  ProviderApiSettings,
} from "../components/ProviderApiSettings";
import { SecretsSettings } from "../components/SecretsSettings";
import { ServerSettings } from "../components/ServerSettings";
import { StorageSettings } from "../components/StorageSettings";
import { TravelSettings } from "../components/TravelSettings";
import { DynamicPluginWidget } from "../components/extensions/DynamicPluginWidget";
import {
  type ResolvedSlotContribution,
  getSlotContributions,
} from "../components/extensions/SlotRegistry";
import type { UseThemeReturn } from "../hooks/useTheme";
import {
  type AppInfo,
  type InstalledExtension,
  type Project,
  type Session,
  core,
} from "../lib/core";
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
    id: "permissions",
    label: "Permissions",
    desc: "Ce que le modèle peut faire dans les nouveaux chats",
    icon: "shield",
  },
  {
    id: "conversations",
    label: "Conversations",
    desc: "Historique et conversations récentes",
    icon: "chat",
  },
  {
    id: "notifications",
    label: "Notifications",
    desc: "Ce pour quoi Locaryn vous dérange",
    icon: "bell",
  },
  { id: "memory", label: "Mémoire", desc: "Ce que Locaryn retient", icon: "memory" },
  { id: "archives", label: "Archives", desc: "Conversations rangées", icon: "archive" },
  {
    id: "appearance",
    label: "Apparence & Langue",
    desc: "Thème, couleur d'accentuation, langue",
    icon: "studio",
  },
];

/**
 * Le point d'extension du compte : une extension y ajoute sa propre
 * sous-section, qu'elle dessine elle-même. L'application ne connaît ni son nom
 * ni ce qu'elle règle.
 */
const SLOT_COMPTE = "settings.account";

/** Une sous-section du compte : native, ou apportée par une extension. */
type AccountPane = AccountSection | `ext:${string}` | "api-keys";

/** Le volet des clés d'API : listé seulement quand l'API fournisseur est
 *  allumée — éteinte, il n'a rien à proposer et encombrerait la liste. */
const VOLET_CLES_API = {
  id: "api-keys" as const,
  label: "Clés d'API",
  desc: "Accès des autres applications aux modèles",
  icon: "key" as IconName,
};

type AccountExtra = {
  key: `ext:${string}`;
  label: string;
  desc: string;
  icon: IconName;
  /** Prête à s'afficher : l'extension tourne sur ce poste. */
  contribution?: ResolvedSlotContribution;
  /** Pas encore installée sur ce poste : l'application propose de le faire. */
  pending?: InstalledExtension;
};

/** Les sous-sections apportées par les extensions, prêtes ou à installer. La
 *  clé suit le nom de l'extension et non son identifiant : installer le
 *  compagnon sur ce poste change l'identifiant, pas l'écran ouvert. */
function accountExtras(extensions: InstalledExtension[]): AccountExtra[] {
  const nameOf = (id: string) => extensions.find((e) => e.id === id)?.name ?? id;
  const ready: AccountExtra[] = getSlotContributions(extensions, SLOT_COMPTE).map((c) => ({
    key: `ext:${nameOf(c.extensionId)}:${c.id}`,
    label: c.label || c.extensionName,
    desc: c.hint ?? c.extensionName,
    icon: (c.icon || "extensions") as IconName,
    contribution: c,
  }));
  const pending: AccountExtra[] = extensions
    .filter((e) => e.enabled && e.device_install_pending)
    .flatMap((e) =>
      (e.ui?.slots ?? [])
        .filter((s) => s.slot === SLOT_COMPTE)
        .map((s) => ({
          key: `ext:${e.name}:${s.id}` as const,
          label: s.label || e.display_name || e.name,
          desc: "À installer sur cet appareil",
          icon: (s.icon || "extensions") as IconName,
          pending: e,
        })),
    );
  return [...ready, ...pending];
}

export type Section =
  | "account"
  | "extensions"
  | "connectors"
  | "projects"
  | "engine"
  | "performance"
  | "huggingface"
  | "server"
  | "secrets"
  | "storage"
  | "about";

type SectionDef = {
  id: Section;
  icon: IconName;
  label: string;
  desc: string;
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
  {
    id: "account",
    icon: "user-circle",
    label: "Compte",
    desc: "Profil local, identité, préférences et mémoire",
  },
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
    label: "Morphs & Skills",
    desc: "Morphs Locaryn, compétences et packs d'agents",
  },
  {
    id: "connectors",
    icon: "server",
    label: "Connecteurs & MCP",
    desc: "Connexions SSH, bases de données et serveurs MCP",
  },
  {
    id: "secrets",
    icon: "lock",
    label: "Coffre",
    desc: "Mots de passe que le modèle utilise sans les voir",
  },
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
  // Le sens du glissement : on descend d'un cran, le menu entre par la droite ;
  // on remonte, il entre par la gauche. Sans ce mouvement, le remplacement
  // d'un menu par l'autre est invisible — on croit que le clic a sauté
  // l'étape et ouvert le contenu directement.
  const [railFrom, setRailFrom] = useState<"deeper" | "back">("deeper");
  const [accountSection, setAccountSection] = useState<AccountPane>("profile");
  const [extensions, setExtensions] = useState<InstalledExtension[]>([]);
  const extras = useMemo(() => accountExtras(extensions), [extensions]);
  const extra = extras.find((x) => x.key === accountSection);
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

  useEffect(() => {
    let cancelled = false;
    const load = () => {
      core
        .listExtensions()
        .then((list) => {
          if (!cancelled) setExtensions(list);
        })
        .catch((e) => {
          console.warn("[Réglages] extensions illisibles :", e);
          if (!cancelled) setExtensions([]);
        });
    };
    load();
    window.addEventListener("locaryn:extensions-changed", load);
    return () => {
      cancelled = true;
      window.removeEventListener("locaryn:extensions-changed", load);
    };
  }, []);

  // La sous-section d'une extension retirée ou désactivée disparaît : on
  // revient au profil plutôt que de montrer un volet vide.
  useEffect(() => {
    if (accountSection.startsWith("ext:") && !extra) setAccountSection("profile");
  }, [accountSection, extra]);

  // L'API fournisseur allumée fait apparaître le volet des clés.
  const [apiActive, setApiActive] = useState(false);
  useEffect(() => {
    let annule = false;
    const lire = () => {
      core
        .providerApiStatus()
        .then((e) => {
          if (!annule) setApiActive(e.enabled);
        })
        .catch(() => {});
    };
    const ouvrirCles = () => {
      setApiActive(true);
      setSection("account");
      setRailLevel("account");
      setAccountSection("api-keys");
      setPaneOpen(true);
    };
    lire();
    window.addEventListener(API_FOURNISSEUR_EVENT, lire);
    window.addEventListener(OUVRIR_CLES_API_EVENT, ouvrirCles);
    return () => {
      annule = true;
      window.removeEventListener(API_FOURNISSEUR_EVENT, lire);
      window.removeEventListener(OUVRIR_CLES_API_EVENT, ouvrirCles);
    };
  }, []);
  useEffect(() => {
    if (accountSection === "api-keys" && !apiActive) setAccountSection("profile");
  }, [accountSection, apiActive]);

  const current = SECTIONS.find((s) => s.id === section) || SECTIONS[0];
  // Dans le compte, le volet porte le nom de la sous-section : le titre doit
  // dire ce qu'on regarde, pas la famille dont ça vient.
  const currentAccount =
    accountSection === "api-keys"
      ? VOLET_CLES_API
      : (ACCOUNT_SECTIONS.find((a) => a.id === accountSection) ?? extra);
  const currentTitle =
    section === "account" && currentAccount
      ? { icon: currentAccount.icon, label: currentAccount.label }
      : { icon: current.icon, label: current.label };
  const remoteEnabled = activeCapabilities.includes("travel-tunnel");

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
          {/* La clé change avec le niveau : sans elle, React réutilise le
              nœud et le navigateur ne rejoue pas l'animation. */}
          <div
            key={railLevel}
            className={
              railFrom === "deeper" ? "locaryn-rail-enter-deeper" : "locaryn-rail-enter-back"
            }
          >
            {railLevel === "account" ? (
              <>
                <button
                  type="button"
                  className="locaryn-settings-back"
                  onClick={() => {
                    setRailFrom("back");
                    setRailLevel("root");
                  }}
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
                  {apiActive && (
                    <button
                      type="button"
                      className={`locaryn-settings-full-item${accountSection === VOLET_CLES_API.id ? " locaryn-active" : ""}`}
                      onClick={() => {
                        setAccountSection(VOLET_CLES_API.id);
                        setPaneOpen(true);
                      }}
                    >
                      <span className="locaryn-settings-full-icon">
                        <Icon name={VOLET_CLES_API.icon} />
                      </span>
                      <span className="locaryn-settings-full-text">
                        <span className="locaryn-settings-full-label">{VOLET_CLES_API.label}</span>
                        <span className="locaryn-settings-full-desc">{VOLET_CLES_API.desc}</span>
                      </span>
                    </button>
                  )}
                  {extras.map((x) => (
                    <button
                      key={x.key}
                      type="button"
                      className={`locaryn-settings-full-item${accountSection === x.key ? " locaryn-active" : ""}`}
                      onClick={() => {
                        setAccountSection(x.key);
                        setPaneOpen(true);
                      }}
                    >
                      <span className="locaryn-settings-full-icon">
                        <Icon name={x.icon} />
                      </span>
                      <span className="locaryn-settings-full-text">
                        <span className="locaryn-settings-full-label">{x.label}</span>
                        <span className="locaryn-settings-full-desc">{x.desc}</span>
                      </span>
                    </button>
                  ))}
                </div>
              </>
            ) : (
              /* Une liste plate : les catégories ajoutaient quatre en-têtes
                 pour douze entrées, et alourdissaient la colonne sans rien
                 classer qu'on ne lise déjà. */
              SECTIONS.map((s) => (
                <button
                  key={s.id}
                  type="button"
                  className={`locaryn-settings-full-item${section === s.id ? " locaryn-active" : ""}`}
                  onClick={() => {
                    setSection(s.id);
                    setPaneOpen(true);
                    if (s.id === "account") {
                      setRailFrom("deeper");
                      setRailLevel("account");
                      // Descendre dans le compte ouvre son premier écran :
                      // un menu sans contenu à côté ne dit rien.
                      setAccountSection("profile");
                    }
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
              ))
            )}
          </div>
        </nav>

        <div
          className="locaryn-settings-full-pane"
          key={section === "account" ? `account:${accountSection}` : section}
        >
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

          {section === "account" && extra?.contribution && (
            <DynamicPluginWidget
              contribution={extra.contribution}
              context={{ surface: SLOT_COMPTE }}
            />
          )}
          {section === "account" && extra?.pending && (
            <DeviceCompanionCard extension={extra.pending} />
          )}
          {section === "account" && accountSection === "api-keys" && <ApiKeysSettings />}
          {section === "account" &&
            !accountSection.startsWith("ext:") &&
            accountSection !== "api-keys" && (
              <AccountView
                embedded
                theme={theme}
                section={accountSection as AccountSection}
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
              <NpuSettings />
              <DecisionSettings />
            </>
          )}
          {section === "huggingface" && <HuggingFaceSettings />}
          {section === "projects" && (
            <>
              <ProjectSettings projects={projects} onArchived={onProjectArchived} />
              <GitSettings />
            </>
          )}
          {section === "extensions" && <ExtensionsSettings />}
          {section === "connectors" && <ConnectorsSettings />}

          {section === "server" && (
            <div className="locaryn-network-layout">
              <div className="locaryn-network-config">
                <ServerSettings />
                <ProviderApiSettings />
                {remoteEnabled && <TravelSettings />}
                <ModeleAutrePc />
                <ConnectionSettings />
              </div>
              <PairingCodes />
            </div>
          )}

          {section === "secrets" && <SecretsSettings />}
          {section === "storage" && <StorageSettings onOpenMarketplace={onOpenMarketplace} />}


          {section === "about" && <AboutSettings />}
        </div>
      </div>
    </section>
  );
}
