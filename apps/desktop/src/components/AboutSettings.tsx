import { check } from "@tauri-apps/plugin-updater";
import type { DownloadEvent } from "@tauri-apps/plugin-updater";
import { useEffect, useState } from "react";
import { type AppInfo, type LlamaRuntimeStatus, type RuntimeCapabilities, core } from "../lib/core";
import { CAPS } from "./EngineSettings";

type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available"; version: string }
  | { kind: "downloading"; progress: number }
  | { kind: "installing" }
  | { kind: "error"; message: string };

/** About page: identity, live capabilities, system paths and licensing. */
export function AboutSettings() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [caps, setCaps] = useState<RuntimeCapabilities | null>(null);
  const [runtime, setRuntime] = useState<LlamaRuntimeStatus | null>(null);
  const [update, setUpdate] = useState<UpdateState>({ kind: "idle" });

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

  async function checkForUpdates() {
    setUpdate({ kind: "checking" });
    try {
      const update = await check();
      if (!update) {
        setUpdate({ kind: "current" });
        return;
      }
      setUpdate({ kind: "available", version: update.version });
    } catch (err) {
      setUpdate({ kind: "error", message: String(err) });
    }
  }

  async function installUpdate() {
    setUpdate({ kind: "installing" });
    try {
      const update = await check();
      if (!update) {
        setUpdate({ kind: "current" });
        return;
      }
      let contentLength = 0;
      await update.downloadAndInstall((ev: DownloadEvent) => {
        if (ev.event === "Started" && ev.data.contentLength) {
          contentLength = ev.data.contentLength;
        } else if (ev.event === "Progress") {
          if (contentLength > 0) {
            setUpdate({
              kind: "downloading",
              progress: Math.min(1, ev.data.chunkLength / contentLength),
            });
          }
        }
      });
      setUpdate({ kind: "installing" });
      // L'installation est terminée : on invite au redémarrage.
      setUpdate({ kind: "current" });
    } catch (err) {
      setUpdate({ kind: "error", message: String(err) });
    }
  }

  const busy =
    update.kind === "checking" || update.kind === "downloading" || update.kind === "installing";

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
        <span className="locaryn-about-version">v{info?.version ?? "0.1.0"}</span>
      </div>

      <div className="locaryn-kv-list" style={{ marginTop: 14 }}>
        <div className="locaryn-kv">
          <span className="locaryn-kv-key">Mises à jour</span>
          <span className="locaryn-kv-val">
            {update.kind === "checking" && "Vérification en cours…"}
            {update.kind === "current" && "À jour"}
            {update.kind === "available" &&
              `Version ${update.version} disponible — cliquez pour installer`}
            {update.kind === "downloading" &&
              `Téléchargement… ${Math.round(update.progress * 100)} %`}
            {update.kind === "installing" && "Installation… l'application va redémarrer"}
            {update.kind === "error" && `Échec : ${update.message}`}
            {update.kind === "idle" && "Vérification manuelle disponible ci-dessous"}
          </span>
        </div>
      </div>

      <div style={{ display: "flex", gap: 10, marginTop: 10, flexWrap: "wrap" }}>
        <button
          type="button"
          className="locaryn-btn locaryn-btn-primary"
          disabled={busy}
          onClick={checkForUpdates}
          title="Vérifier si une nouvelle version de Locaryn est disponible"
        >
          {update.kind === "available"
            ? "Nouvelle version disponible"
            : "Vérifier les mises à jour"}
        </button>
        {update.kind === "available" && (
          <button
            type="button"
            className="locaryn-btn"
            disabled={busy}
            onClick={installUpdate}
            title={`Installer Locaryn ${update.version} et redémarrer`}
          >
            Installer maintenant
          </button>
        )}
        {update.kind === "current" && (
          <button type="button" className="locaryn-btn" onClick={() => setUpdate({ kind: "idle" })}>
            OK
          </button>
        )}
      </div>

      <p className="locaryn-field-hint">
        Un seul outil qui gère lui-même ses moteurs (llama.cpp + stable-diffusion.cpp), exécute vos
        modèles GGUF en local et reste privé par défaut. Pas d'API externe, pas de service à
        assembler.
      </p>

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
