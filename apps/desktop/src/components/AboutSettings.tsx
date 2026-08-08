import { useEffect, useState } from "react";
import { core, type AppInfo, type LlamaRuntimeStatus, type RuntimeCapabilities } from "../lib/core";
import { CAPS } from "./EngineSettings";

/** About page: identity, live capabilities, system paths and licensing. */
export function AboutSettings() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [caps, setCaps] = useState<RuntimeCapabilities | null>(null);
  const [runtime, setRuntime] = useState<LlamaRuntimeStatus | null>(null);

  useEffect(() => {
    let cancelled = false;
    core.appInfo().then((i) => { if (!cancelled) setInfo(i); }).catch(() => {});
    core.runtimeCapabilities().then((c) => { if (!cancelled) setCaps(c); }).catch(() => {});
    core.llamaRuntimeStatus().then((r) => { if (!cancelled) setRuntime(r); }).catch(() => {});
    return () => { cancelled = true; };
  }, []);

  return (
      <div className="lochor-about">
        <div className="lochor-about-hero">
          <span className="lochor-logo-dot" />
          <div>
            <div className="lochor-about-name">Lochor</div>
            <div className="lochor-about-tagline">Moteur d'IA local unifié — chat, agents, vision, RAG et génération d'images, sans dépendre du cloud.</div>
          </div>
          <span className="lochor-about-version">v{info?.version ?? "0.1.0"}</span>
        </div>

        <p className="lochor-field-hint">
          Un seul outil qui gère lui-même ses moteurs (llama.cpp + stable-diffusion.cpp),
          exécute vos modèles GGUF en local et reste privé par défaut. Pas d'API externe,
          pas de service à assembler.
        </p>

        <h3 style={{ marginTop: 24 }}>Ce que fait ce moteur</h3>
        <div className="lochor-caps-grid" style={{ marginTop: 10 }}>
          {CAPS.map((c) => {
            const on = Boolean(caps?.[c.key]);
            return (
              <div key={c.key} className="lochor-cap-chip" style={{ opacity: on ? 1 : 0.5 }} title={c.hint}>
                <span className={`lochor-health-dot ${on ? "lochor-health-ok" : "lochor-health-off"}`} style={{ flex: "0 0 auto" }} />
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontWeight: 600 }}>{c.label}</div>
                  <div className="lochor-field-hint" style={{ margin: 0 }}>{c.hint}</div>
                </div>
              </div>
            );
          })}
        </div>

        <h3 style={{ marginTop: 24 }}>Système</h3>
        <div className="lochor-kv-list">
          <div className="lochor-kv"><span className="lochor-kv-key">Version</span><span className="lochor-kv-val">{info?.version ?? "—"}</span></div>
          <div className="lochor-kv"><span className="lochor-kv-key">Runtime IA</span><span className="lochor-kv-val">llama.cpp {caps?.runtime_version ?? runtime?.version ?? runtime?.pinned ?? "—"} · stable-diffusion.cpp</span></div>
          <div className="lochor-kv"><span className="lochor-kv-key">Architecture</span><span className="lochor-kv-val">Rust + Tauri v2 + React (cœur in-process)</span></div>
          <div className="lochor-kv"><span className="lochor-kv-key">Mode de connexion</span><span className="lochor-kv-val">{info?.mode ?? "local"}</span></div>
          <div className="lochor-kv"><span className="lochor-kv-key">Dossier de données</span><span className="lochor-kv-val lochor-kv-mono">{info?.data_dir ?? "—"}</span></div>
          <div className="lochor-kv"><span className="lochor-kv-key">Base de données</span><span className="lochor-kv-val lochor-kv-mono">{info?.db_path ?? "—"}</span></div>
        </div>

        <h3 style={{ marginTop: 24 }}>Licences open-source</h3>
        <p className="lochor-field-hint">
          Lochor est bâti sur des moteurs sous licence permissive et embarque leurs notices :
        </p>
        <div className="lochor-kv-list">
          <div className="lochor-kv"><span className="lochor-kv-key">llama.cpp · ggml</span><span className="lochor-kv-val">MIT</span></div>
          <div className="lochor-kv"><span className="lochor-kv-key">stable-diffusion.cpp</span><span className="lochor-kv-val">MIT</span></div>
          <div className="lochor-kv"><span className="lochor-kv-key">Lochor (cœur)</span><span className="lochor-kv-val">Apache-2.0 · module entreprise BSL-1.1</span></div>
        </div>
        <p className="lochor-field-hint" style={{ marginTop: 8 }}>
          Les notices complètes sont livrées dans <code>THIRD_PARTY_LICENSES/</code>. Les modèles
          ne sont jamais fournis avec l'app : vous les téléchargez, et chacun garde sa propre licence.
        </p>
      </div>
  );
}
