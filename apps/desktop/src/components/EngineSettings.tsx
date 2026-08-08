import { useEffect, useState } from "react";
import {
  type AppInfo,
  type InferenceConfig,
  type LlamaRuntimeStatus,
  type LoraAdapter,
  type RuntimeCapabilities,
  core,
} from "../lib/core";
import { isNsfwLora } from "../lib/modelSafety";

/** Capabilities shown as chips — shared by the engine panel and the About page. */
export const CAPS: { key: keyof RuntimeCapabilities; label: string; hint: string }[] = [
  { key: "chat", label: "Chat / outils", hint: "Génération de texte + appels d'outils (agent)" },
  { key: "vision", label: "Vision (images)", hint: "Lecture d'images via projecteur mmproj" },
  { key: "embeddings", label: "Embeddings / RAG", hint: "Vecteurs pour la recherche augmentée" },
  {
    key: "image_gen",
    label: "Génération d'images",
    hint: "Diffusion locale (stable-diffusion.cpp)",
  },
  {
    key: "finetune",
    label: "LoRA (adaptateurs)",
    hint: "Applique/permute des LoRA .gguf — l'entraînement se fait en Python",
  },
  {
    key: "distributed",
    label: "Inférence distribuée",
    hint: "Répartit les couches sur plusieurs machines (RPC)",
  },
  {
    key: "speculative_decoding",
    label: "Décodage spéculatif",
    hint: "Modèle draft pour accélérer la génération",
  },
  {
    key: "kv_quant",
    label: "Compression KV",
    hint: "Cache 4/8-bit pour contexte long à VRAM égale",
  },
];

/**
 * Local AI engine: managed llama.cpp runtime, what it can do, and LoRA adapters.
 * Shared by the chat settings popup and the full general-settings view.
 */
export function EngineSettings() {
  const [runtime, setRuntime] = useState<LlamaRuntimeStatus | null>(null);
  const [caps, setCaps] = useState<RuntimeCapabilities | null>(null);
  const [installing, setInstalling] = useState(false);
  const [installPct, setInstallPct] = useState(0);
  const [installStatus, setInstallStatus] = useState("");
  const [installError, setInstallError] = useState<string | null>(null);
  const [engineCfg, setEngineCfg] = useState<InferenceConfig | null>(null);
  const [loraLive, setLoraLive] = useState<LoraAdapter[] | null>(null);
  const [loraPath, setLoraPath] = useState("");
  const [loraNsfwAck, setLoraNsfwAck] = useState(false);

  useEffect(() => {
    let cancelled = false;
    core
      .llamaRuntimeStatus()
      .then((r) => {
        if (!cancelled) setRuntime(r);
      })
      .catch(() => {});
    core
      .runtimeCapabilities()
      .then((c) => {
        if (!cancelled) setCaps(c);
      })
      .catch(() => {});
    core
      .getInferenceConfig()
      .then((c) => {
        if (!cancelled) setEngineCfg(c);
      })
      .catch(() => {});
    core
      .listLoraAdapters()
      .then((l) => {
        if (!cancelled) setLoraLive(l);
      })
      .catch(() => {
        if (!cancelled) setLoraLive(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function installRuntime() {
    setInstalling(true);
    setInstallError(null);
    setInstallPct(0);
    setInstallStatus("Préparation…");
    try {
      const status = await core.setupLlamaRuntime("vulkan", (pct, st) => {
        setInstallPct(pct);
        if (st) setInstallStatus(st);
      });
      setRuntime(status);
      core
        .runtimeCapabilities()
        .then(setCaps)
        .catch(() => {});
    } catch (e) {
      setInstallError(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setInstalling(false);
    }
  }

  async function saveLoraList(next: string[]) {
    if (!engineCfg) return;
    const needsAck = next.some((p) => isNsfwLora(p));
    const consent = needsAck ? loraNsfwAck : undefined;
    const updated = { ...engineCfg, lora_adapters: next };
    setEngineCfg(updated);
    try {
      await core.setInferenceConfig(updated, consent);
    } catch {
      /* keep local state */
    }
  }

  async function setLiveScale(id: number, scale: number) {
    setLoraLive((prev) => prev?.map((a) => (a.id === id ? { ...a, scale } : a)) ?? prev);
    try {
      await core.setLoraAdapters([{ id, scale }]);
    } catch {
      /* server may be down */
    }
  }

  return (
    <div className="locaryn-engine-tab">
      <div className="locaryn-field">
        <label className="locaryn-field-label">Runtime IA (llama.cpp)</label>
        <p className="locaryn-field-hint">
          Le moteur unique qui exécute vos modèles GGUF en local. Locaryn le télécharge et le
          maintient à jour ({runtime?.pinned ?? "…"}, build Vulkan — GPU NVIDIA/AMD/Intel). Pas de
          service externe à assembler.
        </p>
        <div
          className={`locaryn-conn locaryn-conn-${runtime?.up_to_date ? "ok" : runtime?.installed ? "error" : "idle"}`}
          style={{ marginTop: 8 }}
        >
          <span className="locaryn-conn-dot" />
          {runtime == null
            ? "état inconnu"
            : runtime.up_to_date
              ? `Installé et à jour (${runtime.version})`
              : runtime.installed
                ? `Installé (${runtime.version ?? "version inconnue"}) — mise à jour ${runtime.pinned} disponible`
                : "Non installé — requis pour discuter avec un modèle local"}
        </div>
        {installing ? (
          <div style={{ marginTop: 10 }}>
            <div className="locaryn-field-hint">
              {installStatus} {installPct > 0 && installPct < 100 ? `· ${installPct}%` : ""}
            </div>
            <div
              style={{
                height: 6,
                background: "var(--surface)",
                borderRadius: 99,
                marginTop: 6,
                overflow: "hidden",
              }}
            >
              <div
                style={{
                  height: "100%",
                  width: `${installPct}%`,
                  background: "var(--accent)",
                  transition: "width .2s",
                }}
              />
            </div>
          </div>
        ) : (
          !runtime?.up_to_date && (
            <div className="locaryn-field-actions" style={{ marginTop: 12 }}>
              <button type="button" className="locaryn-btn-primary" onClick={installRuntime}>
                {runtime?.installed ? "Mettre à jour le runtime" : "Installer le runtime"}
              </button>
            </div>
          )
        )}
        {installError && (
          <p className="locaryn-field-hint" style={{ color: "var(--danger)", marginTop: 8 }}>
            {installError}
          </p>
        )}
      </div>

      <div className="locaryn-field" style={{ marginTop: 24 }}>
        <label className="locaryn-field-label">Capacités du moteur</label>
        <p className="locaryn-field-hint">
          Un seul moteur couvre toutes ces fonctions. Chaque capacité active dépend du runtime et
          des modèles installés.
        </p>
        <div className="locaryn-caps-grid" style={{ marginTop: 12 }}>
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
        {caps && caps.unavailable.length > 0 && (
          <div style={{ marginTop: 16 }}>
            <div className="locaryn-field-hint" style={{ marginBottom: 6 }}>
              Non couvert par ce moteur (nécessiterait une pile séparée) :
            </div>
            <ul className="locaryn-caps-unavailable">
              {caps.unavailable.map((u) => (
                <li key={u}>{u}</li>
              ))}
            </ul>
          </div>
        )}
      </div>

      {/* LoRA adapters — preload + live hot-swap */}
      <div className="locaryn-field" style={{ marginTop: 24 }}>
        <label className="locaryn-field-label">Adaptateurs LoRA</label>
        <p className="locaryn-field-hint">
          Applique des adaptateurs LoRA (fichiers <code>.gguf</code>) sur le modèle. L'entraînement
          se fait en Python (Unsloth/PEFT) puis <code>convert_lora_to_gguf.py</code> ; ici on les{" "}
          <strong>charge et on ajuste leur intensité à chaud</strong>.
        </p>

        {(engineCfg?.lora_adapters?.length ?? 0) > 0 ? (
          <ul className="locaryn-lora-list">
            {engineCfg!.lora_adapters.map((p, i) => (
              <li key={p + i} className="locaryn-lora-row">
                <span className="locaryn-kv-mono locaryn-lora-path" title={p}>
                  {p}
                </span>
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  onClick={() => saveLoraList(engineCfg!.lora_adapters.filter((_, j) => j !== i))}
                >
                  Retirer
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="locaryn-field-hint" style={{ fontStyle: "italic" }}>
            Aucun adaptateur préchargé.
          </p>
        )}

        <div className="locaryn-field-row" style={{ marginTop: 8 }}>
          <input
            className="locaryn-input"
            placeholder="Chemin vers un adaptateur .gguf"
            value={loraPath}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            onChange={(e) => {
              setLoraPath(e.target.value);
              setLoraNsfwAck(false);
            }}
          />
          <button
            type="button"
            className="locaryn-btn-ghost"
            disabled={!loraPath.trim() || !engineCfg}
            onClick={() => {
              const p = loraPath.trim();
              if (!p || !engineCfg) return;
              if (isNsfwLora(p) && !loraNsfwAck) return;
              if (!engineCfg.lora_adapters.includes(p))
                saveLoraList([...engineCfg.lora_adapters, p]);
              setLoraPath("");
              setLoraNsfwAck(false);
            }}
          >
            Ajouter
          </button>
        </div>
        {isNsfwLora(loraPath) && (
          <label className="locaryn-checkbox-row" style={{ marginTop: 8, color: "var(--danger)" }}>
            <input
              type="checkbox"
              checked={loraNsfwAck}
              onChange={(e) => setLoraNsfwAck(e.target.checked)}
            />
            <span>
              Ce LoRA/embedding est classé NSFW / sans garde-fous. Je prends la responsabilité de
              son usage.
            </span>
          </label>
        )}
        <p className="locaryn-field-hint">
          Ajouter/retirer prend effet au prochain démarrage du serveur (changement de modèle ou
          premier message).
        </p>

        {loraLive && loraLive.length > 0 && (
          <div style={{ marginTop: 16 }}>
            <div className="locaryn-field-hint" style={{ marginBottom: 6 }}>
              Intensité en direct (serveur en cours — sans redémarrage) :
            </div>
            {loraLive.map((a) => (
              <div key={a.id} className="locaryn-lora-live-row">
                <span className="locaryn-kv-mono locaryn-lora-path" title={a.path}>
                  {a.path || `adapter #${a.id}`}
                </span>
                <input
                  type="range"
                  min={0}
                  max={1}
                  step={0.05}
                  value={a.scale}
                  onChange={(e) => setLiveScale(a.id, Number.parseFloat(e.target.value))}
                />
                <span className="locaryn-lora-scale">{a.scale.toFixed(2)}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
