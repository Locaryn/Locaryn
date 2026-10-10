import { useCallback, useEffect, useRef, useState } from "react";
import { type ContextStatus, api } from "../lib/core";

type Props = {
  conversationId: string | null;
  /** Le modèle répond : la jauge se relit à la fin du tour. */
  busy: boolean;
  /** La conversation a été compressée : le fil est à relire. */
  onCompressed: () => void;
};

/** Le temps d'appui qui lance la compression — le même que sur le bureau. */
const APPUI_MS = 1200;
const RELECTURE_MS = 6000;
const RAYON = 11;
const TOUR = 2 * Math.PI * RAYON;

function kilo(n: number): string {
  return n >= 1000
    ? `${(n / 1000).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} k`
    : `${n}`;
}

/**
 * La fenêtre de contexte, en anneau dans la barre du chat.
 *
 * Un toucher ouvre le détail ; un appui maintenu compresse la conversation
 * (les vieux échanges deviennent un résumé écrit par le modèle), comme l'appui
 * long sur la jauge du bureau.
 */
export function ContextGauge({ conversationId, busy, onCompressed }: Props) {
  const [etat, setEtat] = useState<ContextStatus | null>(null);
  const [ouvert, setOuvert] = useState(false);
  const [appui, setAppui] = useState(false);
  const [bulle, setBulle] = useState<string | null>(null);
  const [compression, setCompression] = useState(false);
  const minuterie = useRef<number | null>(null);
  /** Un appui long ne doit pas, en plus, ouvrir le détail au relâcher. */
  const longRef = useRef(false);

  const relire = useCallback(async () => {
    if (!conversationId) {
      setEtat(null);
      return;
    }
    try {
      setEtat(await api.contextStatus(conversationId));
    } catch (e) {
      console.warn("jauge de contexte illisible :", e);
    }
  }, [conversationId]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `busy` déclenche la relecture à la fin d'un tour.
  useEffect(() => {
    void relire();
  }, [relire, busy]);

  useEffect(() => {
    const t = window.setInterval(() => {
      if (document.visibilityState === "visible" && !busy) void relire();
    }, RELECTURE_MS);
    return () => window.clearInterval(t);
  }, [relire, busy]);

  useEffect(() => {
    if (!bulle) return;
    const t = window.setTimeout(() => setBulle(null), 2600);
    return () => window.clearTimeout(t);
  }, [bulle]);

  // Un toucher ailleurs referme le détail, comme tout menu.
  useEffect(() => {
    if (!ouvert) return;
    function ailleurs(e: PointerEvent) {
      if (!(e.target instanceof Element) || !e.target.closest(".lo-ctx")) setOuvert(false);
    }
    document.addEventListener("pointerdown", ailleurs);
    return () => document.removeEventListener("pointerdown", ailleurs);
  }, [ouvert]);

  async function compresser() {
    if (!conversationId || compression) return;
    setOuvert(false);
    setCompression(true);
    setBulle("Compression en cours…");
    try {
      const retires = await api.compressContext(conversationId);
      setBulle(retires > 0 ? "Contexte compressé" : "Rien à compresser pour l'instant");
      onCompressed();
      await relire();
    } catch (e) {
      setBulle(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setCompression(false);
    }
  }

  function commencer() {
    if (!conversationId || compression) return;
    longRef.current = false;
    setAppui(true);
    minuterie.current = window.setTimeout(() => {
      minuterie.current = null;
      longRef.current = true;
      setAppui(false);
      void compresser();
    }, APPUI_MS);
  }

  function relacher() {
    if (minuterie.current) window.clearTimeout(minuterie.current);
    minuterie.current = null;
    setAppui(false);
  }

  if (!conversationId) return null;
  const fenetre = etat?.window ?? null;
  const part = etat && fenetre ? Math.min(1, etat.used / fenetre) : 0;
  const niveau = part >= 0.9 ? "is-full" : part >= 0.75 ? "is-high" : "";

  return (
    <div className="lo-ctx">
      <button
        type="button"
        className={`lo-ctx-ring ${niveau}${appui ? " is-holding" : ""}${compression ? " is-busy" : ""}`}
        aria-label={
          fenetre
            ? `Contexte : ${Math.round(part * 100)} % utilisés. Toucher pour le détail, maintenir pour compresser.`
            : "Contexte de la conversation"
        }
        onPointerDown={commencer}
        onPointerUp={relacher}
        onPointerLeave={relacher}
        onPointerCancel={relacher}
        onContextMenu={(e) => e.preventDefault()}
        onClick={() => {
          if (longRef.current) {
            longRef.current = false;
            return;
          }
          setOuvert((v) => !v);
        }}
      >
        <svg viewBox="0 0 28 28" width="28" height="28" aria-hidden>
          <circle className="lo-ctx-track" cx="14" cy="14" r={RAYON} />
          <circle
            className="lo-ctx-fill"
            cx="14"
            cy="14"
            r={RAYON}
            strokeDasharray={TOUR}
            strokeDashoffset={TOUR * (1 - part)}
          />
          <circle className="lo-ctx-hold" cx="14" cy="14" r={RAYON} strokeDasharray={TOUR} />
        </svg>
      </button>
      {ouvert && (
        <div className="lo-ctx-pop" role="dialog" aria-label="Fenêtre de contexte">
          <p className="lo-ctx-title">Fenêtre de contexte</p>
          {fenetre ? (
            <>
              <p className="lo-ctx-value">
                {kilo(etat?.used ?? 0)} / {kilo(fenetre)} jetons
                <span>{Math.round(part * 100)} %</span>
              </p>
              <div className="lo-ctx-bar">
                <span className={niveau} style={{ width: `${part * 100}%` }} />
              </div>
            </>
          ) : (
            <p className="lo-ctx-value">Le moteur ne dit pas sa fenêtre (modèle arrêté ?).</p>
          )}
          <p className="lo-ctx-note">
            {etat?.messages ?? 0} message(s). Maintenez l'anneau pour compresser : les vieux
            échanges deviennent un résumé, les derniers restent.
          </p>
          <button
            type="button"
            className="lo-ctx-action"
            disabled={!etat?.compressible || compression}
            onClick={() => void compresser()}
          >
            Compresser maintenant
          </button>
        </div>
      )}
      {bulle && <div className="lo-ctx-bulle">{bulle}</div>}
    </div>
  );
}
