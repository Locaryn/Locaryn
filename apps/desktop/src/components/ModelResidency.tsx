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
import { type ModelFit as FitType } from "../lib/core";
import { useI18n } from "../lib/i18n";
type Lang = "fr" | "en";

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

/** La phrase du verdict, reconstituée dans la langue d'affichage à partir
 *  des données structurées du moteur. Fallback : la phrase française
 *  d'origine, quand les parts ne sont pas là (daemon ancien). */
function fitMessage(fit: FitType | null, lang: Lang): string | null {
  if (!fit) return null;
  const parts = fit.message_parts;
  if (!parts || parts.variant === "any") return fit.message;
  if (lang !== "en") return fit.message;
  const n = (v: number | undefined, d = 1) =>
    v === undefined ? "" : v.toLocaleString("en-US", { maximumFractionDigits: d });
  const tok = (v: number | undefined) =>
    v === undefined ? "" : v.toLocaleString("en-US");
  const speed =
    fit.tokens_per_second > 0
      ? `About ${
          fit.tokens_per_second >= 10
            ? Math.round(fit.tokens_per_second)
            : fit.tokens_per_second.toFixed(1)
        } tok/s.`
      : "";
  const quantHint = () => {
    const q = parts.suggested_quant;
    if (q) return ` The ${q} build would fit entirely on the GPU.`;
    if (parts.smaller_context)
      return ` With a ${tok(parts.smaller_context)}-token context it would fit entirely on the GPU.`;
    return "";
  };
  switch (parts.variant) {
    case "gpu":
      return `${n(parts.used_gb)} GB entirely on the GPU (${n(parts.free_vram_gb)} GB free), ${tok(parts.context)}-token context. ${speed}`;
    case "partage":
      return `${parts.gpu_layers ?? 0} of ${parts.total_layers ?? 0} layers fit in the ${n(parts.free_vram_gb)} GB of free VRAM, the rest runs from RAM. ${speed}${quantHint()}`;
    case "ram":
      return `${n(parts.used_gb)} GB in RAM (${n(parts.free_ram_gb)} GB free)${parts.no_gpu ? ", no GPU detected" : ", the VRAM is too small to take a useful share"}. ${speed}`;
    case "risque":
      return `${n(parts.required_gb)} GB needed for ${n(parts.free_ram_gb)} GB free. The system will compensate on disk: severe slowdown, and the app may be killed for lack of memory.`;
    case "refuse_gpu": {
      let text = `Too much VRAM already in use to load this model: ${n(parts.used_gb)} GB to place on the card, ${n(parts.free_vram_gb)} GB free right now, of which ${n(parts.vram_reserve_gb)} GB stay reserved for the system by the chosen caution. Close an app using the GPU (game, 3D software…), or pick a lighter build.`;
      text += quantHint();
      return text;
    }
    case "refuse_ram": {
      let text = `Too much memory already in use to load this model: ${n(parts.required_gb)} GB needed, ${n(parts.free_ram_gb)} GB free right now, of which ${n(parts.ram_reserve_gb)} GB stay reserved for the system by the chosen caution. Close a memory-hungry app, or pick a lighter build.`;
      const q = parts.suggested_quant;
      if (q) text += ` The ${q} build would fit.`;
      else if (parts.smaller_context)
        text += ` With a ${tok(parts.smaller_context)}-token context instead of ${tok(parts.context)}, it would fit.`;
      return text;
    }
    default:
      return fit.message;
  }
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
  const { lang } = useI18n();
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
    const surActivite = (e: Event) => {
      // Regarder le compte à rebours ou ouvrir ce panneau n'est pas se servir
      // du modèle : un clic ici le remettait à 30 min, et le modèle n'était
      // jamais libéré tant qu'on le surveillait.
      if (boxRef.current && e.target instanceof Node && boxRef.current.contains(e.target)) return;
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
      // Avant de vider la mémoire : un plan en cours s'arrête, au lieu de
      // relancer le modèle à l'étape suivante.
      window.dispatchEvent(new Event("locaryn:model-ejecting"));
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
            <div className="locaryn-res-checking">{lang === "en" ? "Checking memory…" : "Vérification de la mémoire…"}</div>
          )}

          {fit && (
            <div className={`locaryn-res-fit ${fit.verdict}`}>
              <div className="locaryn-res-fit-msg">{fitMessage(fit, lang)}</div>

              {/* Les trois postes de mémoire séparés : ils ne se corrigent pas
                  de la même façon. Des poids trop lourds appellent une
                  quantification plus basse, un cache trop gros un contexte
                  plus court. */}
              <dl className="locaryn-res-fit-numbers">
                <div>
                  <dt>{lang === "en" ? "Weights" : "Poids"}</dt>
                  <dd>{fit.size_gb.toFixed(1)} Go</dd>
                </div>
                <div>
                  <dt>Cache {fit.context.toLocaleString(lang === "en" ? "en-US" : "fr-FR")} tok</dt>
                  <dd>{fit.kv_cache_gb.toFixed(1)} Go</dd>
                </div>
                <div>
                  <dt>{lang === "en" ? "GPU layers" : "Couches GPU"}</dt>
                  <dd>
                    {fit.gpu_layers}/{fit.total_layers}
                  </dd>
                </div>
                <div>
                  <dt>{lang === "en" ? "Estimated speed" : "Débit estimé"}</dt>
                  <dd>
                    {fit.tokens_per_second >= 10
                      ? Math.round(fit.tokens_per_second)
                      : fit.tokens_per_second.toFixed(1)}{" "}
                    tok/s
                  </dd>
                </div>
              </dl>

              {/* Un chiffre dont on ignore les hypothèses n'est pas
                  vérifiable : elles sont là, repliées. */}
              {fit.assumptions.length > 0 && (
                <details className="locaryn-res-fit-why">
                  <summary>{lang === "en" ? "What this estimate relies on" : "Sur quoi repose cette estimation"}</summary>
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
                      {lang === "en" ? "Force anyway" : "Forcer quand même"}
                    </button>
                  )
                ) : (
                  <button
                    type="button"
                    className="locaryn-res-load"
                    onClick={() => void confirmLoad(false)}
                    disabled={busy !== null}
                  >
                    {lang === "en" ? (busy === "load" ? "Loading…" : "Load and keep in memory") : (busy === "load" ? "Chargement…" : "Charger et garder en mémoire")}
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
