// Moitié gauche de la barre du bas : quel modèle de chat est en mémoire, et
// la main dessus.
//
// Le moteur garde les poids résidents tant qu'il tourne — parler au modèle ne
// le recharge pas. Ce qui manquait, c'est la visibilité et le contrôle : sans
// épinglage, le superviseur décharge après un temps d'inactivité, et on repaie
// le chargement en revenant. Un modèle chargé ici est épinglé : il reste
// jusqu'à ce qu'on l'éjecte.
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
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))} s`;
  const m = Math.ceil(seconds / 60);
  return `${m} min`;
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
  const [busy, setBusy] = useState<null | "fit" | "load" | "eject">(null);
  const [error, setError] = useState<string | null>(null);
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
  /** Quand l'état affiché a été lu : le compte à rebours avance d'ici. */
  const luA = useRef(Date.now());
  const boxRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    try {
      const s = await core.modelResidency();
      luA.current = Date.now();
      setStatus(s);
    } catch {
      // Une barre d'état qui hurle parce qu'un sondage a raté serait pire que
      // le silence : on garde le dernier état connu.
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 5_000);
    // Une réponse qui commence ou finit change ce que la barre doit dire
    // (compte à rebours masqué ou relancé) : pas besoin d'attendre le sondage.
    const now = () => void refresh();
    window.addEventListener("locaryn:generation", now);
    return () => {
      clearInterval(t);
      window.removeEventListener("locaryn:generation", now);
    };
  }, [refresh]);

  // Entre deux sondages, le compte à rebours avance seul, seconde par seconde.
  const [, setTick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => setTick((n) => n + 1), 1_000);
    return () => clearInterval(t);
  }, []);
  /** Secondes avant que le modèle quitte la mémoire faute d'utilisation. */
  const resteAvantLiberation =
    status && status.idle_timeout_seconds > 0
      ? Math.max(
          0,
          status.idle_timeout_seconds - status.idle_seconds - (Date.now() - luA.current) / 1000,
        )
      : null;

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
      setStatus(await core.loadChatModel(picked, force));
      setOpen(false);
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
      setStatus(res);
      window.dispatchEvent(new Event("locaryn:model-ejected"));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  const dot = stateDot(status, busy === "load" || status?.loading === true);
  const loaded = status?.loaded === true;
  const enService = status?.busy === true;
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
        // La fenêtre que le moteur applique vraiment — celle de la jauge du
        // chat. La valeur réglée pouvait différer (contexte automatique,
        // plafond du moteur) et la barre annonçait 122K pour 8K réels.
        status?.context
          ? `ctx ${contextLabel(status.context)}`
          : config && config.context_length > 0
            ? `ctx ${contextLabel(config.context_length)}`
            : null,
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
            : status?.loading
              ? "Chargement du modèle…"
              : "Aucun modèle chargé"}
      </button>

      {facts.length > 0 && (
        <span className="locaryn-res-facts">
          {facts.map((f) => (
            <span key={f}>{f}</span>
          ))}
        </span>
      )}

      {/* Le compte à rebours d'éjection : toujours visible quand le modèle
          attend, masqué pendant qu'il répond à la personne. Le travail
          d'arrière-plan (tri de la mémoire) ne l'arrête pas. */}
      {loaded && !status?.pinned && !enService && resteAvantLiberation !== null && (
        <span
          className={`locaryn-res-hint${resteAvantLiberation <= 300 ? " is-proche" : ""}`}
          title="Le modèle quitte la mémoire après 30 minutes sans utilisation. Un message, ou n'importe quelle action dans l'application, relance le décompte."
        >
          éjection dans {minutes(resteAvantLiberation)}
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
                    {busy === "load" ? "Chargement…" : "Charger et garder en mémoire"}
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
