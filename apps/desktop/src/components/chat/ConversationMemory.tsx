import { useEffect, useRef, useState } from "react";
import { plainMemory } from "../../lib/plainLanguage";

/**
 * La mémoire de la conversation, pour le mode simplifié.
 *
 * Le mode expert a sa jauge en jetons ; ici, des mots : combien de pages
 * tiennent encore, et le moment où le modèle commence à se perdre. Rien ne
 * s'affiche tant que la taille de la mémoire est inconnue (aucun modèle
 * chargé) : un pourcentage inventé mentirait.
 */
export function ConversationMemory({
  used,
  window: fenetre,
  compressing,
  onCompress,
  onNewChat,
}: {
  used: number;
  window: number | null;
  compressing: boolean;
  /** Résume les anciens échanges ; `undefined` sans conversation ouverte. */
  onCompress?: () => void;
  onNewChat?: () => void;
}) {
  const [ouvert, setOuvert] = useState(false);
  const boite = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!ouvert) return;
    const dehors = (e: MouseEvent) => {
      if (boite.current && !boite.current.contains(e.target as Node)) setOuvert(false);
    };
    document.addEventListener("mousedown", dehors);
    return () => document.removeEventListener("mousedown", dehors);
  }, [ouvert]);

  if (fenetre === null || fenetre <= 0) return null;
  const m = plainMemory(used, fenetre);
  const teinte = m.level === "pleine" ? "danger" : m.level === "bientot" ? "warn" : "ok";
  const r = 7;
  const c = 2 * Math.PI * r;

  return (
    <div className="locaryn-usage locaryn-memory" ref={boite} data-simple-only>
      <button
        type="button"
        className={`locaryn-usage-pill locaryn-memory-pill is-${teinte}`}
        onClick={() => setOuvert((o) => !o)}
        aria-expanded={ouvert}
        title={`${m.short} : ${m.percent} % de la mémoire utilisée`}
      >
        <svg width="18" height="18" viewBox="0 0 18 18" aria-hidden="true">
          <circle cx="9" cy="9" r={r} className="locaryn-usage-rail" />
          <circle
            cx="9"
            cy="9"
            r={r}
            className="locaryn-usage-arc"
            strokeDasharray={`${(m.percent / 100) * c} ${c}`}
            transform="rotate(-90 9 9)"
          />
        </svg>
        <span>{m.short}</span>
      </button>
      {ouvert && (
        <div className="locaryn-usage-pop" role="dialog" aria-label="Mémoire de la conversation">
          <div className="locaryn-usage-pop-head">
            <strong>Mémoire de la conversation</strong>
            <span>{m.percent} %</span>
          </div>
          <div className={`locaryn-usage-bar is-${teinte}`}>
            <span style={{ width: `${m.percent}%` }} />
          </div>
          <p className="locaryn-memory-detail">{m.detail}</p>
          {(m.level === "bientot" || m.level === "pleine") && (
            <div className="locaryn-memory-actions">
              {onCompress && (
                <button
                  type="button"
                  className="locaryn-btn"
                  disabled={compressing}
                  onClick={onCompress}
                >
                  {compressing ? "Résumé en cours…" : "Résumer la conversation"}
                </button>
              )}
              {onNewChat && (
                <button
                  type="button"
                  className="locaryn-btn locaryn-btn-ghost"
                  onClick={() => {
                    setOuvert(false);
                    onNewChat();
                  }}
                >
                  Nouvelle conversation
                </button>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
