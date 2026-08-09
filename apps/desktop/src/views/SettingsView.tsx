import { useEffect, useState } from "react";
import { AboutSettings } from "../components/AboutSettings";
import { CautionSettings } from "../components/CautionSettings";
import { ConnectionSettings } from "../components/ConnectionSettings";
import { ConnectorsSettings } from "../components/ConnectorsSettings";
import { EngineSettings } from "../components/EngineSettings";
import { ExtensionsSettings } from "../components/ExtensionsSettings";
import { HuggingFaceSettings } from "../components/HuggingFaceSettings";
import { ImageSettings } from "../components/ImageSettings";
import { PerformancePanel } from "../components/PerformancePanel";
import { ServerSettings } from "../components/ServerSettings";
import { StorageSettings } from "../components/StorageSettings";
import { TravelSettings } from "../components/TravelSettings";
import type { UseThemeReturn } from "../hooks/useTheme";
import { ACCENT_PRESETS } from "../hooks/useTheme";
import { type AppInfo, type Project, core } from "../lib/core";
import { getPendingInstall, subscribeDeepLink } from "../lib/deepLink";
import { DO_NOT_TRANSLATE, LANGUAGES, useI18n } from "../lib/i18n";
import { ProjectSettings } from "./ProjectSettings";

type Section =
  | "engine"
  | "performance"
  | "image"
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
  onProjectArchived?: (p: Project) => void;
  /** Jump to the model marketplace (models are managed there, not here). */
  onOpenMarketplace?: () => void;
};

const SECTIONS: { id: Section; icon: string; label: string; desc: string }[] = [
  {
    id: "engine",
    icon: "⚙",
    label: "Moteur IA",
    desc: "Runtime llama.cpp, capacités, adaptateurs LoRA",
  },
  { id: "performance", icon: "⚡", label: "Performance", desc: "GPU, cache KV, contexte, offload" },
  {
    id: "image",
    icon: "🎨",
    label: "Image",
    desc: "Qualité et résolution par défaut des générations",
  },
  {
    id: "huggingface",
    icon: "🤗",
    label: "HuggingFace",
    desc: "Token pour les dépôts restreints (modèles gated)",
  },
  {
    id: "projects",
    icon: "📁",
    label: "Projets",
    desc: "Autorisations, base de connaissances, archivage",
  },
  {
    id: "extensions",
    icon: "🧩",
    label: "Extensions",
    desc: "Plugins Claude Code, Gemini CLI, OpenCode, MCP",
  },
  {
    id: "connectors",
    icon: "🔌",
    label: "Connecteurs",
    desc: "Serveurs SSH et MCP ajoutés à la main",
  },
  { id: "appearance", icon: "🎨", label: "Apparence", desc: "Couleur d'accentuation, thème" },
  { id: "language", icon: "🌍", label: "Langue", desc: "Langue de l'interface" },
  {
    id: "server",
    icon: "🌐",
    label: "Partage réseau",
    desc: "Rendre cette machine accessible aux autres postes",
  },
  {
    id: "storage",
    icon: "💾",
    label: "Stockage",
    desc: "Emplacement des modèles, espace disque, nettoyage",
  },
  { id: "about", icon: "ℹ", label: "À propos", desc: "Version, licences, système" },
];

/**
 * Full-page general settings, reached from the left navigation. Distinct from
 * the compact chat settings popup: this covers the whole application (engine,
 * projects, extensions, appearance, storage) while the popup only carries the
 * chat-scoped knobs. Shared sections are the same components in both.
 */
export function SettingsView({ theme, projects, onProjectArchived, onOpenMarketplace }: Props) {
  const { settings, updateAccent, resetTheme } = theme;
  const { lang, setLang } = useI18n();
  const [section, setSection] = useState<Section>("engine");
  const [info, setInfo] = useState<AppInfo | null>(null);

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

  return (
    <section className="locaryn-view-container">
      <div className="locaryn-view-header">
        <h2>Paramètres de l'application</h2>
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
              <span className="locaryn-settings-full-icon">{s.icon}</span>
              <span className="locaryn-settings-full-text">
                <span className="locaryn-settings-full-label">{s.label}</span>
                <span className="locaryn-settings-full-desc">{s.desc}</span>
              </span>
            </button>
          ))}
        </nav>

        <div className="locaryn-settings-full-pane">
          <h3 className="locaryn-settings-full-title">
            {current.icon} {current.label}
          </h3>

          {section === "engine" && <EngineSettings />}
          {section === "performance" && (
            <>
              <PerformancePanel />
              <CautionSettings />
            </>
          )}
          {section === "image" && <ImageSettings />}
          {section === "huggingface" && <HuggingFaceSettings />}
          {section === "projects" && (
            <ProjectSettings projects={projects} onArchived={onProjectArchived} />
          )}
          {section === "extensions" && <ExtensionsSettings />}
          {section === "connectors" && <ConnectorsSettings />}

          {section === "appearance" && (
            <div className="locaryn-field">
              <label className="locaryn-field-label">Couleur d'accentuation</label>
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
                      <span className="locaryn-swatch-check">✓</span>
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
            <>
              <ServerSettings />
              <TravelSettings />
              <ConnectionSettings />
            </>
          )}

          {section === "storage" && <StorageSettings onOpenMarketplace={onOpenMarketplace} />}

          {section === "language" && (
            <div className="locaryn-field">
              <label className="locaryn-field-label">Langue de l'interface</label>
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
                    {lang === l.id && <span className="locaryn-lang-check">✓</span>}
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
