/**
 * La présentation du premier lancement, commune au bureau, au téléphone et au
 * web.
 *
 * Chaque application donne ses étapes : une scène animée, un titre, deux
 * phrases. Les scènes illustrent ce qui surprend dans Locaryn — un modèle qui
 * tourne sur la machine, un dossier de travail choisi au départ, des niveaux
 * de permission, une conversation qu'on glisse vers la corbeille. Le
 * mouvement reste calme et se fige quand le système demande moins
 * d'animations. La matière vit dans `packages-ui/tokens/presentation.css`.
 */

import { type ReactNode, useCallback, useEffect, useRef, useState } from "react";

export type PresentationScene =
  | "local"
  | "chat"
  | "folder"
  | "permissions"
  | "extensions"
  | "drag"
  | "phone";

export interface PresentationStep {
  scene: PresentationScene;
  title: string;
  body: string;
}

interface Props {
  steps: PresentationStep[];
  /** Clé de stockage : une fois vue, la présentation ne revient plus seule. */
  storageKey: string;
  /** Rejouer depuis les réglages, même déjà vue. */
  forceOpen?: boolean;
  onClose?: () => void;
}

/** Déjà vue ? Un stockage indisponible (navigation privée) vaut « jamais vue ». */
function dejaVue(key: string): boolean {
  try {
    return window.localStorage.getItem(key) === "1";
  } catch {
    return false;
  }
}

function retenir(key: string): void {
  try {
    window.localStorage.setItem(key, "1");
  } catch {
    // Sans stockage, elle reviendra au prochain lancement : rien de grave.
  }
}

export function Presentation({ steps, storageKey, forceOpen = false, onClose }: Props) {
  const [ouverte, setOuverte] = useState(() => forceOpen || !dejaVue(storageKey));
  const [index, setIndex] = useState(0);
  const touche = useRef<number | null>(null);

  useEffect(() => {
    if (forceOpen) {
      setIndex(0);
      setOuverte(true);
    }
  }, [forceOpen]);

  const fermer = useCallback(() => {
    retenir(storageKey);
    setOuverte(false);
    onClose?.();
  }, [storageKey, onClose]);

  const derniere = index === steps.length - 1;
  const suivant = useCallback(() => {
    if (derniere) fermer();
    else setIndex((i) => i + 1);
  }, [derniere, fermer]);
  const precedent = useCallback(() => setIndex((i) => Math.max(0, i - 1)), []);

  useEffect(() => {
    if (!ouverte) return;
    const clavier = (e: KeyboardEvent) => {
      if (e.key === "Escape") fermer();
      else if (e.key === "ArrowRight" || e.key === "Enter") suivant();
      else if (e.key === "ArrowLeft") precedent();
    };
    window.addEventListener("keydown", clavier);
    return () => window.removeEventListener("keydown", clavier);
  }, [ouverte, fermer, suivant, precedent]);

  if (!ouverte || steps.length === 0) return null;
  const etape = steps[index];

  return (
    <div
      className="lo-pres"
      role="dialog"
      aria-modal="true"
      aria-label="Présentation de Locaryn"
      onTouchStart={(e) => {
        touche.current = e.touches[0]?.clientX ?? null;
      }}
      onTouchEnd={(e) => {
        const depart = touche.current;
        const fin = e.changedTouches[0]?.clientX;
        touche.current = null;
        if (depart === null || fin === undefined || Math.abs(fin - depart) < 48) return;
        if (fin < depart) suivant();
        else precedent();
      }}
    >
      <button type="button" className="lo-pres-skip" onClick={fermer}>
        Passer
      </button>
      <div className="lo-pres-card">
        <div className="lo-pres-stage" key={`scene-${index}`} aria-hidden="true">
          <Scene name={etape.scene} />
        </div>
        <div className="lo-pres-text" key={`texte-${index}`}>
          <span className="lo-pres-kicker">
            {index + 1} / {steps.length}
          </span>
          <h2 className="lo-pres-title">{etape.title}</h2>
          <p className="lo-pres-body">{etape.body}</p>
          <Controles
            index={index}
            total={steps.length}
            onGo={setIndex}
            onPrev={precedent}
            onNext={suivant}
          />
        </div>
      </div>
    </div>
  );
}

function Controles({
  index,
  total,
  onGo,
  onPrev,
  onNext,
}: {
  index: number;
  total: number;
  onGo: (i: number) => void;
  onPrev: () => void;
  onNext: () => void;
}) {
  return (
    <div className="lo-pres-controls">
      <div className="lo-pres-dots" role="tablist" aria-label="Étapes">
        {Array.from({ length: total }, (_, i) => (
          <button
            // biome-ignore lint/suspicious/noArrayIndexKey: les étapes sont fixes, l'indice est leur identité.
            key={i}
            type="button"
            role="tab"
            aria-selected={i === index}
            aria-label={`Étape ${i + 1}`}
            className={`lo-pres-dot${i === index ? " is-on" : ""}`}
            onClick={() => onGo(i)}
          />
        ))}
      </div>
      <div className="lo-pres-buttons">
        {index > 0 && (
          <button type="button" className="lo-pres-btn lo-pres-btn-ghost" onClick={onPrev}>
            Précédent
          </button>
        )}
        <button type="button" className="lo-pres-btn lo-pres-btn-primary" onClick={onNext}>
          {index === total - 1 ? "Commencer" : "Suivant"}
        </button>
      </div>
    </div>
  );
}

// ── Scènes ────────────────────────────────────────────────────────────────
// Dessins au trait, une seule teinte d'accent, comme le reste de l'interface.

function Scene({ name }: { name: PresentationScene }) {
  const scenes: Record<PresentationScene, ReactNode> = {
    local: <SceneLocal />,
    chat: <SceneChat />,
    folder: <SceneFolder />,
    permissions: <ScenePermissions />,
    extensions: <SceneExtensions />,
    drag: <SceneDrag />,
    phone: <ScenePhone />,
  };
  return (
    <svg className={`lo-scene lo-scene-${name}`} viewBox="0 0 240 180" role="presentation">
      {scenes[name]}
    </svg>
  );
}

/** Le modèle se charge dans la machine : une barre se remplit, puis un point vert. */
function SceneLocal() {
  return (
    <g>
      <rect className="ls-line" x="40" y="34" width="160" height="100" rx="8" />
      <path className="ls-line" d="M24 146h192" />
      <rect className="ls-fill-soft" x="62" y="58" width="116" height="40" rx="6" />
      <rect className="ls-track" x="74" y="108" width="92" height="6" rx="3" />
      <rect className="ls-accent ls-load" x="74" y="108" width="92" height="6" rx="3" />
      <circle className="ls-accent ls-ready" cx="120" cy="78" r="6" />
      <path className="ls-faint" d="M78 72h20M78 84h34" />
    </g>
  );
}

/** Une question, une réflexion qui se replie, une réponse. */
function SceneChat() {
  return (
    <g>
      <rect className="ls-fill-soft ls-pop ls-d0" x="112" y="28" width="96" height="22" rx="11" />
      <rect className="ls-line ls-pop ls-d1" x="32" y="62" width="120" height="18" rx="6" />
      <path className="ls-faint ls-pop ls-d1" d="M42 71h48" />
      <rect className="ls-accent-soft ls-fold" x="32" y="86" width="120" height="34" rx="6" />
      <rect className="ls-fill-soft ls-pop ls-d2" x="32" y="128" width="150" height="26" rx="13" />
    </g>
  );
}

/** Une conversation s'attache à un dossier ; un cadenas dit que le choix est fait. */
function SceneFolder() {
  return (
    <g>
      <path
        className="ls-line"
        d="M44 60h44l10 10h82a6 6 0 0 1 6 6v62a6 6 0 0 1-6 6H44a6 6 0 0 1-6-6V66a6 6 0 0 1 6-6z"
      />
      <rect className="ls-fill-soft ls-attach" x="82" y="18" width="76" height="24" rx="6" />
      <rect className="ls-accent ls-lock" x="110" y="96" width="20" height="16" rx="3" />
      <path className="ls-line ls-lock" d="M114 96v-6a6 6 0 0 1 12 0v6" />
    </g>
  );
}

/** Cinq paliers ; le curseur glisse, et un bandeau d'accord monte au-dessus du champ. */
function ScenePermissions() {
  return (
    <g>
      <path className="ls-track-line" d="M40 50h160" />
      {[40, 80, 120, 160, 200].map((x) => (
        <circle key={x} className="ls-tick" cx={x} cy="50" r="4" />
      ))}
      <circle className="ls-accent ls-knob" cx="40" cy="50" r="8" />
      <rect className="ls-accent-soft ls-banner" x="44" y="96" width="152" height="24" rx="6" />
      <rect className="ls-line" x="36" y="128" width="168" height="28" rx="8" />
    </g>
  );
}

/** Trois briques — morph, skill, connecteur — viennent se brancher sur l'application. */
function SceneExtensions() {
  return (
    <g>
      <rect className="ls-line" x="92" y="62" width="56" height="56" rx="10" />
      <circle className="ls-accent" cx="120" cy="90" r="6" />
      <rect
        className="ls-fill-soft ls-snap ls-from-left"
        x="34"
        y="78"
        width="40"
        height="24"
        rx="6"
      />
      <rect
        className="ls-fill-soft ls-snap ls-from-top"
        x="100"
        y="16"
        width="40"
        height="24"
        rx="6"
      />
      <rect
        className="ls-fill-soft ls-snap ls-from-right"
        x="166"
        y="78"
        width="40"
        height="24"
        rx="6"
      />
      <path className="ls-faint ls-wire" d="M74 90h18M120 40v22M148 90h18" />
    </g>
  );
}

/** Une ligne quitte la liste, les autres se resserrent, la corbeille s'allume. */
function SceneDrag() {
  return (
    <g>
      <rect className="ls-line ls-bin" x="60" y="14" width="120" height="24" rx="6" />
      <rect className="ls-fill-soft" x="44" y="56" width="152" height="18" rx="5" />
      <rect className="ls-fill-soft ls-collapse" x="44" y="80" width="152" height="18" rx="5" />
      <rect className="ls-fill-soft ls-shift" x="44" y="104" width="152" height="18" rx="5" />
      <rect className="ls-fill-soft ls-shift" x="44" y="128" width="152" height="18" rx="5" />
      <rect className="ls-accent-soft ls-ghost" x="56" y="80" width="128" height="18" rx="5" />
    </g>
  );
}

/** Un téléphone lit le code de l'ordinateur, puis la coche. */
function ScenePhone() {
  return (
    <g>
      <rect className="ls-line" x="26" y="40" width="96" height="70" rx="6" />
      <rect className="ls-fill-soft" x="54" y="54" width="40" height="40" rx="3" />
      <path className="ls-faint" d="M60 60h10v10H60zM78 60h10v10H78zM60 78h10v10H60z" />
      <rect className="ls-line" x="150" y="30" width="58" height="112" rx="10" />
      <path className="ls-accent ls-scan" d="M158 60h42" />
      <path className="ls-accent-line ls-check" d="M168 92l9 9 16-18" />
    </g>
  );
}
