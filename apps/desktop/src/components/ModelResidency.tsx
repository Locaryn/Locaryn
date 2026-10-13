// Moitié gauche de la barre du bas : quel modèle de chat est en mémoire, et
// la main dessus.
//
// Le moteur garde les poids résidents tant qu'il tourne — parler au modèle ne
// le recharge pas. Ce qui manquait, c'est la visibilité et le contrôle : sans
// épinglage, le superviseur décharge après un temps d'inactivité, et on repaie
// le chargement en revenant.
//
// Charger n'épingle donc plus. Un modèle choisi ici repart après le délai
// d'inactivité, et revient au message suivant ; « garder » est un bouton, pas
// la conséquence cachée d'un choix de modèle — sinon un modèle chargé un soir
// tenait encore la VRAM le lendemain matin.
//
// Le garde-fou mémoire s'affiche *avant* le chargement, jamais après. Un
// avertissement qui arrive quand la machine rame déjà n'est pas un
// avertissement.

import { useCallback, useEffect, useRef, useState } from "react";
import {
  type InferenceConfig,
  type ModelFit,
  type ModelMetric,
  type ResidencyStatus,
  core,
} from "../lib/core";
import { notifierSysteme } from "../lib/osNotify";
import { findMetric, formatRate, formatSpeed } from "./SpeedBadge";

/** Le point de l'indicateur : couleur et libellé disent la même chose, pour
 *  que l'information ne repose pas seulement sur la couleur. */
function stateDot(status: ResidencyStatus | null, busy: boolean): { cls: string; title: string } {
  if (busy) return { cls: "locaryn-res-dot loading", title: "Chargement en cours" };
  if (!status?.loaded) return { cls: "locaryn-res-dot idle", title: "Aucun modèle en mémoire" };
  if (status.pinned) return { cls: "locaryn-res-dot pinned", title: "En mémoire, épinglé" };
  return { cls: "locaryn-res-dot warm", title: "En mémoire, déchargement automatique" };
}

/**
 * La quantification, lue dans le nom du fichier.
 *
 * Le moteur ne la renvoie pas : elle est dans le nom, par convention GGUF
 * (`…-Q4_K_M.gguf`, `…-IQ3_XS.gguf`). Absente, on n'invente rien.
 */
function quantization(model: string | null | undefined): string | null {
  if (!model) return null;
  const m = model.match(/\b(I?Q\d(?:_[A-Z0-9]+)*|F16|F32|BF16)\b/i);
  return m ? m[1].toUpperCase() : null;
}

/** Le débit mesuré pour ce modèle sur cette machine, s'il y en a un. */
function debitMesure(metrics: ModelMetric[], model: string): string | null {
  const metric = findMetric(metrics, model, "chat");
  return metric ? formatSpeed(metric) : null;
}

/** Le contexte, écrit court : 32768 se lit mieux en « 32K ». */
function contextLabel(tokens: number): string {
  if (tokens >= 1024) return `${Math.round(tokens / 1024)}K`;
  return String(tokens);
}

function minutes(seconds: number): string {
  const m = Math.round(seconds / 60);
  return m <= 1 ? "1 min" : `${m} min`;
}

function isChatModel(name: string): boolean {
  const n = name.toLowerCase();
  if (n.endsWith(".pth") || n.endsWith(".pt") || n.endsWith(".onnx") || n.endsWith(".bin"))
    return false;
  if (
    n.includes("tts") ||
    n.includes("kokoro") ||
    n.includes("piper") ||
    n.includes("whisper") ||
    n.includes("customvoice") ||
    n.includes("speech")
  ) {
    return false;
  }
  if (
    n.includes("embed") ||
    n.includes("embedding") ||
    n.includes("bge-") ||
    n.includes("minilm") ||
    n.includes("rerank")
  ) {
    return false;
  }
  if (
    n.includes("stable-diffusion") ||
    n.includes("sdxl") ||
    n.includes("flux") ||
    n.includes("vae")
  ) {
    return false;
  }
  return true;
}

export function ModelResidency() {
  const [status, setStatus] = useState<ResidencyStatus | null>(null);
  const [open, setOpen] = useState(false);
  const [models, setModels] = useState<string[]>([]);
  const [picked, setPicked] = useState<string | null>(null);
  const [fit, setFit] = useState<ModelFit | null>(null);
  const [busy, setBusy] = useState<null | "fit" | "load" | "eject" | "pin">(null);
  const [error, setError] = useState<string | null>(null);
  /** La case du panneau : garder le modèle n'est pas le défaut. */
  const [garder, setGarder] = useState(false);
  /** Le dernier état lu : il dit si le délai vient de rendre le modèle. */
  const dernier = useRef<ResidencyStatus | null>(null);
  /* La quantification, le débit et le contexte accompagnent le nom du modèle
     dans la barre d'état : trois chiffres en mono, lus d'un coup d'œil. */
  const [metrics, setMetrics] = useState<ModelMetric[]>([]);
  /** La vitesse de génération de la dernière réponse, mesurée par le moteur. */
  const [live, setLive] = useState<number | null>(null);
  useEffect(() => {
    const onSpeed = (e: Event) => {
      const detail = (e as CustomEvent<{ generation: number }>).detail;
      if (detail && detail.generation > 0) setLive(detail.generation);
    };
    window.addEventListener("locaryn:speed", onSpeed);
    return () => window.removeEventListener("locaryn:speed", onSpeed);
  }, []);
  const [config, setConfig] = useState<InferenceConfig | null>(null);
  /** Secondes avant que le modèle quitte la mémoire faute d'utilisation. */
  const resteAvantLiberation =
    status && status.idle_timeout_seconds > 0
      ? Math.max(0, status.idle_timeout_seconds - status.idle_seconds)
      : null;
  const boxRef = useRef<HTMLDivElement>(null);

  /** Le même état à deux endroits : l'écran, et la mémoire du dernier lu. */
  const majStatus = useCallback((suivant: ResidencyStatus) => {
    dernier.current = suivant;
    setStatus(suivant);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const suivant = await core.modelResidency();
      const avant = dernier.current;
      majStatus(suivant);
      // Le délai d'inactivité vient de rendre le modèle : le dire, puisque
      // c'est la seule trace d'un déchargement qui s'est décidé sans nous. Le
      // contrôle du temps d'inactivité évite d'annoncer « longue inactivité »
      // pour un arrêt demandé à la main quelques secondes plus tôt.
      const renduParLeDelai =
        avant?.loaded === true &&
        avant.idle_timeout_seconds > 0 &&
        avant.idle_seconds >= avant.idle_timeout_seconds - 60;
      if (renduParLeDelai && !suivant.loaded && avant.model) {
        void notifierSysteme(
          "Modèle libéré",
          `${avant.model} a quitté la mémoire après une longue inactivité.`,
          "model_lifecycle",
          avant.idle_timeout_seconds * 1000,
        );
      }
    } catch {
      // Une barre d'état qui hurle parce qu'un sondage a raté serait pire que
      // le silence : on garde le dernier état connu.
    }
  }, [majStatus]);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 15_000);
    return () => clearInterval(t);
  }, [refresh]);

  // Taper ou cliquer dans l'application, c'est s'en servir : le délai
  // d'inactivité repart, au plus une fois par minute. Sans cela, seul l'envoi
  // d'un message comptait, et le modèle pouvait être libéré pendant qu'on
  // lisait sa réponse ou qu'on écrivait la suite.
  useEffect(() => {
    let dernier = 0;
    const surActivite = () => {
      const maintenant = Date.now();
      if (maintenant - dernier < 60_000) return;
      dernier = maintenant;
      void core
        .signalerActivite()
        .then(() => refresh())
        .catch(() => {});
    };
    window.addEventListener("keydown", surActivite);
    window.addEventListener("pointerdown", surActivite);
    return () => {
      window.removeEventListener("keydown", surActivite);
      window.removeEventListener("pointerdown", surActivite);
    };
  }, [refresh]);

  useEffect(() => {
    core
      .listModelMetrics()
      .then(setMetrics)
      .catch(() => {
        // Pas de mesure : la barre montre le reste, elle n'invente pas un débit.
      });
    core
      .getInferenceConfig()
      .then(setConfig)
      .catch(() => {
        // Pas de configuration lisible : le contexte reste muet.
      });
  }, []);

  // Fermer au clic extérieur : le panneau recouvre le chat, il ne doit pas
  // rester ouvert par inadvertance.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  async function openPicker() {
    setOpen(true);
    setError(null);
    setPicked(null);
    setFit(null);
    try {
      const endpoint = status?.endpoint ?? "http://127.0.0.1:8080";
      const all = await core.listModels(endpoint);
      setModels(all.filter(isChatModel));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  async function pick(model: string) {
    setPicked(model);
    setFit(null);
    setError(null);
    setBusy("fit");
    try {
      setFit(await core.checkModelFit(model));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function confirmLoad(force: boolean) {
    if (!picked) return;
    setBusy("load");
    setError(null);
    try {
      let res = await core.loadChatModel(picked, force);
      // La case n'épingle qu'à la demande : le chargement seul laisse le délai
      // d'inactivité faire son travail.
      if (garder) res = await core.setModelPinned(true);
      majStatus(res);
      setOpen(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function basculerEpinglage() {
    if (!status) return;
    setBusy("pin");
    setError(null);
    try {
      majStatus(await core.setModelPinned(!status.pinned));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function eject() {
    setBusy("eject");
    setError(null);
    try {
      const res = await core.ejectChatModel();
      majStatus(res);
      window.dispatchEvent(new Event("locaryn:model-ejected"));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  const dot = stateDot(status, busy === "load");
  const loaded = status?.loaded === true;
  // Trois faits sur le modèle chargé, chacun affiché seulement s'il existe :
  // un chiffre absent est une information, un chiffre inventé n'en est pas une.
  const facts = loaded
    ? [
        quantization(status?.model),
        live !== null
          ? `${formatRate(live)} jetons/s`
          : status?.model
            ? debitMesure(metrics, status.model)
            : null,
        config ? `ctx ${contextLabel(config.context_length)}` : null,
      ].filter((f): f is string => Boolean(f))
    : [];

  return (
    <div className="locaryn-residency" ref={boxRef}>
      <span className={dot.cls} title={dot.title} aria-hidden="true" />

      <button
        type="button"
        className="locaryn-res-name"
        onClick={() => (open ? setOpen(false) : void openPicker())}
        title={
          loaded
            ? (status?.model ?? "modèle inconnu")
            : busy === "load" && picked
              ? `Chargement de ${picked}`
              : "Choisir un modèle à charger en mémoire"
        }
        aria-expanded={open}
      >
        {busy === "load"
          ? `Chargement de ${picked ?? "…"}`
          : loaded
            ? (status?.model ?? "modèle inconnu")
            : "Aucun modèle chargé"}
      </button>

      {facts.length > 0 && (
        <span className="locaryn-res-facts">
          {facts.map((f) => (
            <span key={f}>{f}</span>
          ))}
        </span>
      )}

      {/* Un compte à rebours permanent annonçait une libération à quelqu'un en
          plein travail. Il n'apparaît qu'à l'approche de l'échéance, quand
          l'application n'a vraiment pas servi. */}
      {loaded &&
        !status?.pinned &&
        resteAvantLiberation !== null &&
        resteAvantLiberation <= 300 && (
          <span
            className="locaryn-res-hint"
            title="Le modèle quitte la mémoire après une période sans utilisation. Un message, ou n'importe quelle action dans l'application, le garde chargé."
          >
            libéré dans {minutes(resteAvantLiberation)} sans utilisation
          </span>
        )}

      {loaded && (
        <button
          type="button"
          className={`locaryn-res-eject locaryn-res-pin${status?.pinned ? " garde" : ""}`}
          onClick={() => void basculerEpinglage()}
          disabled={busy !== null}
          title={
            status?.pinned
              ? "Le modèle reste en mémoire jusqu'à ce que vous l'éjectiez. Cliquez pour le rendre au délai d'inactivité."
              : "Empêcher le déchargement automatique après une longue inactivité."
          }
        >
          {busy === "pin" ? "…" : status?.pinned ? "Gardé" : "Garder"}
        </button>
      )}

      {error && !open && (
        <span className="locaryn-res-hint" title={error}>
          échec : {error}
        </span>
      )}

      {loaded && (
        <button
          type="button"
          className="locaryn-res-eject"
          onClick={() => void eject()}
          disabled={busy !== null}
          title="Décharger le modèle et rendre la mémoire"
        >
          {busy === "eject" ? "…" : "Éjecter"}
        </button>
      )}

      {open && (
        <dialog open className="locaryn-res-panel" aria-label="Charger un modèle">
          <div className="locaryn-res-panel-head">Charger un modèle en mémoire</div>

          <div className="locaryn-res-list">
            {models.length === 0 && <div className="locaryn-res-empty">Aucun modèle installé.</div>}
            {models.map((m) => (
              <button
                type="button"
                key={m}
                className={`locaryn-res-item${picked === m ? " picked" : ""}`}
                onClick={() => void pick(m)}
                disabled={busy === "load"}
              >
                {m}
              </button>
            ))}
          </div>

          {busy === "fit" && (
            <div className="locaryn-res-checking">Vérification de la mémoire…</div>
          )}

          {fit && (
            <div className={`locaryn-res-fit ${fit.verdict}`}>
              <div className="locaryn-res-fit-msg">{fit.message}</div>

              {/* Les trois postes de mémoire séparés : ils ne se corrigent pas
                  de la même façon. Des poids trop lourds appellent une
                  quantification plus basse, un cache trop gros un contexte
                  plus court. */}
              <dl className="locaryn-res-fit-numbers">
                <div>
                  <dt>Poids</dt>
                  <dd>{fit.size_gb.toFixed(1)} Go</dd>
                </div>
                <div>
                  <dt>Cache {fit.context.toLocaleString("fr-FR")} jetons</dt>
                  <dd>{fit.kv_cache_gb.toFixed(1)} Go</dd>
                </div>
                <div>
                  <dt>Couches GPU</dt>
                  <dd>
                    {fit.gpu_layers}/{fit.total_layers}
                  </dd>
                </div>
                <div>
                  <dt>Débit estimé</dt>
                  <dd>
                    {fit.tokens_per_second >= 10
                      ? Math.round(fit.tokens_per_second)
                      : fit.tokens_per_second.toFixed(1)}{" "}
                    jetons/s
                  </dd>
                </div>
              </dl>

              {/* Un chiffre dont on ignore les hypothèses n'est pas
                  vérifiable : elles sont là, repliées. */}
              {fit.assumptions.length > 0 && (
                <details className="locaryn-res-fit-why">
                  <summary>Sur quoi repose cette estimation</summary>
                  <ul>
                    {fit.assumptions.map((line) => (
                      <li key={line}>{line}</li>
                    ))}
                  </ul>
                </details>
              )}
              <label className="locaryn-res-keep">
                <input
                  type="checkbox"
                  checked={garder}
                  onChange={(e) => setGarder(e.target.checked)}
                />
                Garder en mémoire jusqu'à ce que je l'éjecte
              </label>
              <p className="locaryn-res-keep-why">
                Sans cette case, le modèle quitte la mémoire après{" "}
                {minutes(status?.idle_timeout_seconds || 1800)} sans utilisation — un message, ou
                n'importe quelle action dans l'application, repart le délai.
              </p>

              <div className="locaryn-res-actions">
                {fit.verdict === "refuse" ? (
                  fit.overridable && (
                    <button
                      type="button"
                      className="locaryn-res-force"
                      onClick={() => void confirmLoad(true)}
                      disabled={busy !== null}
                    >
                      Forcer quand même
                    </button>
                  )
                ) : (
                  <button
                    type="button"
                    className="locaryn-res-load"
                    onClick={() => void confirmLoad(false)}
                    disabled={busy !== null}
                  >
                    {busy === "load"
                      ? "Chargement…"
                      : garder
                        ? "Charger et garder en mémoire"
                        : "Charger"}
                  </button>
                )}
              </div>
            </div>
          )}

          {error && <div className="locaryn-res-error">{error}</div>}
        </dialog>
      )}
    </div>
  );
}
