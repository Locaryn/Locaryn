import { open } from "@tauri-apps/plugin-shell";
import { type Update, check } from "@tauri-apps/plugin-updater";
import { useEffect, useRef, useState } from "react";
import {
  type AppInfo,
  type LlamaRuntimeStatus,
  type RuntimeCapabilities,
  core,
  coreMode,
} from "../lib/core";
import { CAPS } from "./EngineSettings";

/** Page GitHub des versions — la référence pour l'installation manuelle. */
const RELEASES_URL = "https://github.com/Locaryn/Locaryn/releases/latest";

type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "uptodate" }
  | { kind: "available"; update: Update }
  | {
      kind: "downloading";
      update: Update;
      downloaded: number;
      total: number | null;
    }
  | { kind: "error"; message: string };

function formatBytes(n: number): string {
  if (n < 1024) return `${n} o`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} Ko`;
  return `${(n / (1024 * 1024)).toFixed(1)} Mo`;
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

/**
 * Le plugin renvoie parfois une phrase très courte (« request failed ») alors
 * que la cause est utile pour la personne : GitHub bloqué, manifeste absent ou
 * signature invalide. On garde le détail, mais on donne d'abord une action.
 */
function updateErrorMessage(e: unknown): string {
  const raw = errorMessage(e)
    .replace(/^Error:\s*/i, "")
    .trim();
  // L'URL est retirée avant tout test : elle finit par « latest.json », donc
  // le motif qui cherchait « json » ou « manifest » l'y trouvait toujours. Une
  // panne réseau — « error sending request for url (…/latest.json) » — était
  // ainsi annoncée comme un manifeste absent, et envoyait corriger une chaîne
  // de publication qui n'avait rien.
  const cause = raw.replace(/https?:\/\/\S+/g, "").toLowerCase();
  if (/signature|public key|pubkey|verify|verification/.test(cause)) {
    return `Le manifeste GitHub est accessible, mais sa signature n'est pas acceptée. Installez la dernière version depuis GitHub, puis réessayez. (${raw})`;
  }
  // Le transport d'abord : une requête qui ne part pas ne dit rien du contenu.
  if (
    /network|connect|dns|timeout|timed out|tls|certificate|request failed|sending request|offline|unreachable|refused|proxy/.test(
      cause,
    )
  ) {
    return `GitHub est inaccessible depuis cette machine. Vérifiez la connexion, le proxy ou le pare-feu, puis réessayez. (${raw})`;
  }
  if (/404|not found|no such|missing/.test(cause)) {
    return `Le manifeste de mise à jour est introuvable sur GitHub. La release doit publier « latest.json ». (${raw})`;
  }
  return raw || "La vérification a échoué sans détail fourni par le système.";
}

/** About page: identity, live capabilities, system paths and licensing. */
export function AboutSettings() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [caps, setCaps] = useState<RuntimeCapabilities | null>(null);
  const [runtime, setRuntime] = useState<LlamaRuntimeStatus | null>(null);
  const [updateState, setUpdateState] = useState<UpdateState>({ kind: "idle" });
  const checkedRef = useRef(false);

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

  // Silent check on first load, once the platform is known. Linux has no
  // automatic updater (Tauri supports Windows/macOS only) — it always falls
  // back to the releases page, so no network call is wasted there.
  useEffect(() => {
    if (!info || checkedRef.current) return;
    checkedRef.current = true;
    if (info.platform === "linux") return;
    void runCheck();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [info]);

  const updaterSupported = coreMode === "tauri" && (info ? info.platform !== "linux" : true);

  async function runCheck() {
    setUpdateState({ kind: "checking" });
    try {
      const update = await check();
      if (!update) {
        setUpdateState({ kind: "uptodate" });
        return;
      }
      setUpdateState({ kind: "available", update });
    } catch (e) {
      setUpdateState({
        kind: "error",
        message: updateErrorMessage(e),
      });
    }
  }

  async function install(update: Update) {
    setUpdateState({
      kind: "downloading",
      update,
      downloaded: 0,
      total: null,
    });
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          setUpdateState((s) =>
            s.kind === "downloading" ? { ...s, total: event.data.contentLength ?? s.total } : s,
          );
        } else if (event.event === "Progress") {
          setUpdateState((s) =>
            s.kind === "downloading"
              ? { ...s, downloaded: s.downloaded + event.data.chunkLength }
              : s,
          );
        }
      });
      // L'installation terminée, l'application se relance toute seule.
    } catch (e) {
      setUpdateState({
        kind: "error",
        message: updateErrorMessage(e),
      });
    }
  }

  function openReleases() {
    void open(RELEASES_URL);
  }

  const downloading = updateState.kind === "downloading";

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
            {updaterSupported ? (
              updateState.kind === "checking" ? (
                <span className="locaryn-update-status">Vérification de la dernière version…</span>
              ) : updateState.kind === "uptodate" ? (
                <span className="locaryn-update-status locaryn-update-ok">
                  Vous êtes à jour ({info?.version ? `v${info.version}` : "version inconnue"}).
                </span>
              ) : updateState.kind === "available" ? (
                <span className="locaryn-update-status">
                  Version <b>v{updateState.update.version}</b> disponible (vous avez v
                  {info?.version ?? "inconnue"}).
                </span>
              ) : updateState.kind === "downloading" ? (
                <span className="locaryn-update-status">
                  Téléchargement de v{updateState.update.version}…{" "}
                  {updateState.total != null
                    ? `${formatBytes(updateState.downloaded)} / ${formatBytes(updateState.total)}`
                    : formatBytes(updateState.downloaded)}
                </span>
              ) : updateState.kind === "error" ? (
                <span className="locaryn-update-status locaryn-update-error">
                  Vérification impossible : {updateState.message}
                </span>
              ) : (
                <span className="locaryn-update-status">
                  Le bouton vérifie si une version plus récente existe sur GitHub.
                </span>
              )
            ) : (
              <span className="locaryn-update-status">
                {coreMode !== "tauri"
                  ? "Mode aperçu navigateur : l'updater natif est désactivé."
                  : "Sur Linux, l'application ne se met pas à jour toute seule : téléchargez la dernière version depuis GitHub."}
              </span>
            )}
            {info && (
              <span className="locaryn-update-detail">
                {info.platform} · {info.arch}
              </span>
            )}
          </div>
          <div className="locaryn-update-actions">
            {updaterSupported && (
              <button
                type="button"
                className="locaryn-btn-primary"
                disabled={updateState.kind === "checking" || downloading}
                onClick={() => {
                  if (updateState.kind === "available") {
                    void install(updateState.update);
                  } else {
                    void runCheck();
                  }
                }}
              >
                {updateState.kind === "available"
                  ? "Installer et redémarrer"
                  : downloading
                    ? "Téléchargement…"
                    : "Vérifier les mises à jour"}
              </button>
            )}
            <button type="button" className="locaryn-btn-ghost" onClick={openReleases}>
              Voir les versions sur GitHub
            </button>
          </div>
        </div>
        {downloading && updateState.total != null && updateState.total > 0 && (
          <div className="locaryn-update-progress">
            <div
              className="locaryn-update-progress-fill"
              style={{
                width: `${Math.min(100, (updateState.downloaded / updateState.total) * 100)}%`,
              }}
            />
          </div>
        )}
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
