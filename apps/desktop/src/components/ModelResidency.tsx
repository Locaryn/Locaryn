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
import { type ModelFit, type ResidencyStatus, core } from "../lib/core";

/** Le point de l'indicateur : couleur et libellé disent la même chose, pour
 *  que l'information ne repose pas seulement sur la couleur. */
function stateDot(status: ResidencyStatus | null, busy: boolean): { cls: string; title: string } {
  if (busy) return { cls: "locaryn-res-dot loading", title: "Chargement en cours" };
  if (!status?.loaded) return { cls: "locaryn-res-dot idle", title: "Aucun modèle en mémoire" };
  if (status.pinned) return { cls: "locaryn-res-dot pinned", title: "En mémoire, épinglé" };
  return { cls: "locaryn-res-dot warm", title: "En mémoire, déchargement automatique" };
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
  const [busy, setBusy] = useState<null | "fit" | "load" | "eject">(null);
  const [error, setError] = useState<string | null>(null);
  const boxRef = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await core.modelResidency());
    } catch {
      // Une barre d'état qui hurle parce qu'un sondage a raté serait pire que
      // le silence : on garde le dernier état connu.
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 15_000);
    return () => clearInterval(t);
  }, [refresh]);

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

  const dot = stateDot(status, busy === "load");
  const loaded = status?.loaded === true;

  return (
    <div className="locaryn-residency" ref={boxRef}>
      <span className={dot.cls} title={dot.title} aria-hidden="true" />

      <button
        type="button"
        className="locaryn-res-name"
        onClick={() => (open ? setOpen(false) : void openPicker())}
        title={
          loaded
            ? status?.pinned
              ? "En mémoire et épinglé — il y reste jusqu'à l'éjection"
              : `En mémoire — déchargement après ${minutes(status?.idle_timeout_seconds ?? 1800)} sans activité`
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

      {loaded && !status?.pinned && (
        <span className="locaryn-res-hint">
          libéré dans{" "}
          {minutes(Math.max(0, (status?.idle_timeout_seconds ?? 0) - (status?.idle_seconds ?? 0)))}
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
