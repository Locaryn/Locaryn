import { LoSwitch } from "@locaryn/ui-core";
import { open } from "@tauri-apps/plugin-shell";
import { useEffect, useState } from "react";
import {
  type AppInfo,
  type LlamaRuntimeStatus,
  type RuntimeCapabilities,
  core,
  coreMode,
} from "../lib/core";
import { CAPS } from "./EngineSettings";
import { demanderVerification } from "./UpdateDialog";

/** Page GitHub des versions — la référence pour l'installation manuelle. */
const RELEASES_URL = "https://github.com/Locaryn/Locaryn/releases/latest";

/** About page: identity, live capabilities, system paths and licensing. */
export function AboutSettings() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [caps, setCaps] = useState<RuntimeCapabilities | null>(null);
  const [runtime, setRuntime] = useState<LlamaRuntimeStatus | null>(null);
  const [appBetaChannel, setAppBetaChannel] = useState<boolean>(() => {
    return localStorage.getItem("locaryn_app_beta_channel") === "true";
  });
  const [showBetaMorphs, setShowBetaMorphs] = useState<boolean>(() => {
    return localStorage.getItem("locaryn_show_beta_morphs") !== "false";
  });

  useEffect(() => {
    let cancelled = false;
    core
      .appInfo()
      .then((i) => {
        if (!cancelled) setInfo(i);
      })
      .catch(() => {});
    core
      .runtimeCapabilities()
      .then((c) => {
        if (!cancelled) setCaps(c);
      })
      .catch(() => {});
    core
      .llamaRuntimeStatus()
      .then((r) => {
        if (!cancelled) setRuntime(r);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const updaterSupported = coreMode === "tauri" && (info ? info.platform !== "linux" : true);

  function openReleases() {
    void open(RELEASES_URL);
  }

  return (
    <div className="locaryn-about">
      <div className="locaryn-about-hero">
        <span className="locaryn-logo-dot" />
        <div>
          <div className="locaryn-about-name">Locaryn</div>
          <div className="locaryn-about-tagline">
            Moteur d'IA local unifié — chat, agents, vision, RAG et génération d'images, sans
            dépendre du cloud.
          </div>
        </div>
        <span className="locaryn-about-version">
          {info?.version ? `v${info.version}` : "version inconnue"}
        </span>
      </div>

      <p className="locaryn-field-hint">
        Un seul outil qui gère lui-même ses moteurs (llama.cpp + stable-diffusion.cpp), exécute vos
        modèles GGUF en local et reste privé par défaut. Pas d'API externe, pas de service à
        assembler.
      </p>

      <h3 style={{ marginTop: 24 }}>Mises à jour</h3>
      <div className="locaryn-update-card">
        <div className="locaryn-update-row">
          <div className="locaryn-update-info">
            {/* L'etat d'une verification vit dans la fenetre de mise a jour,
                qui s'ouvre par-dessus. Le repeter ici obligeait a maintenir
                deux machines a etats sur le meme sujet, et lancait deux
                requetes au demarrage. */}
            <span className="locaryn-update-status">
              {updaterSupported
                ? "Le bouton vérifie si une version plus récente existe sur GitHub."
                : coreMode !== "tauri"
                  ? "Mode aperçu navigateur : l'updater natif est désactivé."
                  : "Sur Linux, l'application ne se met pas à jour toute seule : téléchargez la dernière version depuis GitHub."}
            </span>
            {info && (
              <span className="locaryn-update-detail">
                v{info.version} · {info.platform} · {info.arch}
              </span>
            )}
          </div>
          <div className="locaryn-update-actions">
            {updaterSupported && (
              <button
                type="button"
                className="locaryn-btn-primary"
                onClick={() => demanderVerification()}
              >
                Vérifier les mises à jour
              </button>
            )}
            <button type="button" className="locaryn-btn-ghost" onClick={openReleases}>
              Voir les versions sur GitHub
            </button>
          </div>
        </div>

        <div
          style={{
            marginTop: 16,
            paddingTop: 16,
            borderTop: "1px solid var(--border)",
            display: "flex",
            flexDirection: "column",
            gap: 16,
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              gap: 16,
            }}
          >
            <div>
              <div style={{ fontWeight: 600, fontSize: 13, color: "var(--text)" }}>
                Canal de mise à jour Bêta (Testeurs)
              </div>
              <p className="locaryn-field-hint" style={{ margin: 0, fontSize: 12 }}>
                Recevoir les pré-versions expérimentales de l'application Locaryn pour tester les
                nouvelles fonctionnalités avant leur sortie officielle.
              </p>
            </div>
            <LoSwitch
              checked={appBetaChannel}
              onChange={(checked) => {
                setAppBetaChannel(checked);
                localStorage.setItem("locaryn_app_beta_channel", checked ? "true" : "false");
              }}
              label="Activer le canal de mise à jour Bêta de l'application"
            />
          </div>

          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              gap: 16,
            }}
          >
            <div>
              <div style={{ fontWeight: 600, fontSize: 13, color: "var(--text)" }}>
                Afficher les Morphs en version Bêta / Non testée
              </div>
              <p className="locaryn-field-hint" style={{ margin: 0, fontSize: 12 }}>
                Autoriser l'affichage et l'installation des Morphs en version préliminaire ou non
                vérifiés (marqués en ambre) dans le store.
              </p>
            </div>
            <LoSwitch
              checked={showBetaMorphs}
              onChange={(checked) => {
                setShowBetaMorphs(checked);
                localStorage.setItem("locaryn_show_beta_morphs", checked ? "true" : "false");
                window.dispatchEvent(new CustomEvent("locaryn-settings-changed"));
              }}
              label="Afficher les Morphs en version Bêta dans le catalogue"
            />
          </div>
        </div>
      </div>

      <h3 style={{ marginTop: 24 }}>Ce que fait ce moteur</h3>
      <div className="locaryn-caps-grid" style={{ marginTop: 10 }}>
        {CAPS.map((c) => {
          const on = Boolean(caps?.[c.key]);
          return (
            <div
              key={c.key}
              className="locaryn-cap-chip"
              style={{ opacity: on ? 1 : 0.5 }}
              title={c.hint}
            >
              <span
                className={`locaryn-health-dot ${on ? "locaryn-health-ok" : "locaryn-health-off"}`}
                style={{ flex: "0 0 auto" }}
              />
              <div style={{ minWidth: 0 }}>
                <div style={{ fontWeight: 600 }}>{c.label}</div>
                <div className="locaryn-field-hint" style={{ margin: 0 }}>
                  {c.hint}
                </div>
              </div>
            </div>
          );
        })}
      </div>

      <h3 style={{ marginTop: 24 }}>Système</h3>
      <div className="locaryn-kv-list">
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">Version</span>
          <span className="locaryn-kv-val">{info?.version ?? "—"}</span>
        </div>
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">Runtime IA</span>
          <span className="locaryn-kv-val">
            llama.cpp {caps?.runtime_version ?? runtime?.version ?? runtime?.pinned ?? "—"} ·
            stable-diffusion.cpp
          </span>
        </div>
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">Architecture</span>
          <span className="locaryn-kv-val">Rust + Tauri v2 + React (cœur in-process)</span>
        </div>
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">Mode de connexion</span>
          <span className="locaryn-kv-val">{info?.mode ?? "local"}</span>
        </div>
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">Dossier de données</span>
          <span className="locaryn-kv-val locaryn-kv-mono">{info?.data_dir ?? "—"}</span>
        </div>
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">Base de données</span>
          <span className="locaryn-kv-val locaryn-kv-mono">{info?.db_path ?? "—"}</span>
        </div>
      </div>

      <h3 style={{ marginTop: 24 }}>Licences open-source</h3>
      <p className="locaryn-field-hint">
        Locaryn est bâti sur des moteurs sous licence permissive et embarque leurs notices :
      </p>
      <div className="locaryn-kv-list">
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">llama.cpp · ggml</span>
          <span className="locaryn-kv-val">MIT</span>
        </div>
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">stable-diffusion.cpp</span>
          <span className="locaryn-kv-val">MIT</span>
        </div>
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">Locaryn (cœur)</span>
          <span className="locaryn-kv-val">Apache-2.0 · module entreprise BSL-1.1</span>
        </div>
      </div>
      <p className="locaryn-field-hint" style={{ marginTop: 8 }}>
        Les notices complètes sont livrées dans <code>THIRD_PARTY_LICENSES/</code>. Les modèles ne
        sont jamais fournis avec l'app : vous les téléchargez, et chacun garde sa propre licence.
      </p>
    </div>
  );
}
