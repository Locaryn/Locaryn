/**
 * La fenêtre de contexte en anneau.
 *
 * Chaque poste (messages, prompt système, outils…) est un arc de l'anneau ;
 * le reste du cercle, c'est l'espace libre. Au centre, la part occupée — et,
 * quand on survole ou touche un poste, la part de ce poste seul. La liste à
 * côté répond au même geste : l'arc et la ligne s'allument ensemble.
 *
 * Partagé par le bureau et le téléphone : la même mesure se lit pareil partout.
 */

import { useState } from "react";

export interface ContextRingPart {
  key: string;
  label: string;
  tokens: number;
  /** Une couleur CSS (de préférence une variable du thème). */
  color: string;
}

export interface ContextRingProps {
  parts: ContextRingPart[];
  /** La fenêtre du moteur, en jetons. */
  window: number;
  /** Ce qui reste hors fenêtre mais existe (outils différés) : affiché à part. */
  extra?: { label: string; tokens: number } | null;
}

const TAILLE = 132;
const EPAISSEUR = 12;
const RAYON = (TAILLE - EPAISSEUR) / 2;
const TOUR = 2 * Math.PI * RAYON;
/** L'espace entre deux arcs, en fraction du tour. */
const ECART = 0.006;

export function formatTokens(n: number): string {
  return n >= 1000
    ? `${(n / 1000).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} k`
    : `${n}`;
}

function pourcent(part: number): string {
  if (part > 0 && part < 0.01) return "< 1 %";
  return `${Math.round(part * 100)} %`;
}

export function ContextRing({ parts, window: fenetre, extra }: ContextRingProps) {
  const [actif, setActif] = useState<string | null>(null);
  const utilise = parts.reduce((a, p) => a + p.tokens, 0);
  const libre = Math.max(0, fenetre - utilise);
  const base = Math.max(fenetre, utilise, 1);

  // Les arcs, posés bout à bout depuis midi.
  let depart = 0;
  const arcs = parts
    .filter((p) => p.tokens > 0)
    .map((p) => {
      const part = p.tokens / base;
      const longueur = Math.max(0, part - ECART) * TOUR;
      const arc = { ...p, part, longueur, decalage: -depart * TOUR };
      depart += part;
      return arc;
    });

  const focus = parts.find((p) => p.key === actif);
  const centre = focus ? focus.tokens / base : utilise / base;

  return (
    <div className="lo-ring">
      <div className="lo-ring-figure">
        <svg
          viewBox={`0 0 ${TAILLE} ${TAILLE}`}
          width={TAILLE}
          height={TAILLE}
          role="img"
          aria-label={`${pourcent(utilise / base)} de la fenêtre utilisés`}
        >
          <circle
            className="lo-ring-piste"
            cx={TAILLE / 2}
            cy={TAILLE / 2}
            r={RAYON}
            strokeWidth={EPAISSEUR}
          />
          <g transform={`rotate(-90 ${TAILLE / 2} ${TAILLE / 2})`}>
            {arcs.map((a) => (
              <circle
                key={a.key}
                className={`lo-ring-arc${actif && actif !== a.key ? " is-eteint" : ""}`}
                cx={TAILLE / 2}
                cy={TAILLE / 2}
                r={RAYON}
                strokeWidth={EPAISSEUR}
                stroke={a.color}
                strokeDasharray={`${a.longueur} ${TOUR}`}
                strokeDashoffset={a.decalage}
                onPointerEnter={() => setActif(a.key)}
                onPointerLeave={() => setActif(null)}
              />
            ))}
          </g>
        </svg>
        <div className="lo-ring-centre" aria-hidden>
          <strong>{pourcent(centre)}</strong>
          <span>{focus ? focus.label : `${formatTokens(utilise)} / ${formatTokens(fenetre)}`}</span>
        </div>
      </div>
      <ul className="lo-ring-legende">
        {parts.map((p) => (
          <li key={p.key}>
            <button
              type="button"
              className={actif === p.key ? "is-actif" : undefined}
              onPointerEnter={() => setActif(p.key)}
              onPointerLeave={() => setActif(null)}
              onFocus={() => setActif(p.key)}
              onBlur={() => setActif(null)}
              onClick={() => setActif((v) => (v === p.key ? null : p.key))}
            >
              <span className="lo-ring-pastille" style={{ background: p.color }} />
              <span className="lo-ring-nom">{p.label}</span>
              <span className="lo-ring-n">{formatTokens(p.tokens)}</span>
            </button>
          </li>
        ))}
        <li className="is-libre">
          <span className="lo-ring-ligne">
            <span className="lo-ring-pastille is-vide" />
            <span className="lo-ring-nom">Libre</span>
            <span className="lo-ring-n">{formatTokens(libre)}</span>
          </span>
        </li>
        {extra && extra.tokens > 0 && (
          <li className="is-hors">
            <span className="lo-ring-ligne">
              <span className="lo-ring-pastille is-hors" />
              <span className="lo-ring-nom">{extra.label}</span>
              <span className="lo-ring-n">{formatTokens(extra.tokens)}</span>
            </span>
          </li>
        )}
      </ul>
    </div>
  );
}
