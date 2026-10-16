import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { type EtatSoutien, core, coreMode } from "../lib/core";

/** Envoyé par la conversation à la fin d'une réponse. */
export const REPONSE_FINIE_EVENT = "locaryn:reponse-finie";

const COULEURS = ["#6ebe91", "#f2c94c", "#eb5757", "#56ccf2", "#bb6bd9", "#f2994a", "#ffffff"];

/**
 * Des confettis plein écran, dessinés sur une toile posée par-dessus tout et
 * retirée à la fin. Rien quand la personne demande moins d'animations.
 */
function confettis(nombre: number, salves = 1) {
  if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
  const toile = document.createElement("canvas");
  toile.className = "locaryn-confettis";
  const dpr = window.devicePixelRatio || 1;
  toile.width = window.innerWidth * dpr;
  toile.height = window.innerHeight * dpr;
  document.body.appendChild(toile);
  const ctx = toile.getContext("2d");
  if (!ctx) {
    toile.remove();
    return;
  }
  ctx.scale(dpr, dpr);
  type P = {
    x: number;
    y: number;
    vx: number;
    vy: number;
    angle: number;
    rotation: number;
    taille: number;
    couleur: string;
    rond: boolean;
  };
  const particules: P[] = [];
  const lancer = (n: number) => {
    for (let i = 0; i < n; i++) {
      // De deux canons en bas des côtés, et une pluie depuis le haut.
      const cote = i % 3;
      const depuisHaut = cote === 2;
      particules.push({
        x: depuisHaut
          ? Math.random() * window.innerWidth
          : cote === 0
            ? -10
            : window.innerWidth + 10,
        y: depuisHaut ? -20 - Math.random() * 200 : window.innerHeight * 0.75,
        vx: depuisHaut
          ? (Math.random() - 0.5) * 4
          : (cote === 0 ? 1 : -1) * (6 + Math.random() * 9),
        vy: depuisHaut ? 2 + Math.random() * 3 : -(12 + Math.random() * 10),
        angle: Math.random() * Math.PI * 2,
        rotation: (Math.random() - 0.5) * 0.3,
        taille: 6 + Math.random() * 7,
        couleur: COULEURS[Math.floor(Math.random() * COULEURS.length)],
        rond: Math.random() < 0.3,
      });
    }
  };
  lancer(nombre);
  for (let s = 1; s < salves; s++) setTimeout(() => lancer(nombre), s * 650);
  const debut = performance.now();
  const duree = 3200 + salves * 650;
  const image = (t: number) => {
    ctx.clearRect(0, 0, window.innerWidth, window.innerHeight);
    const fondu = Math.max(0, 1 - Math.max(0, t - debut - (duree - 900)) / 900);
    for (const p of particules) {
      p.vy += 0.32;
      p.vx *= 0.99;
      p.x += p.vx;
      p.y += p.vy;
      p.angle += p.rotation;
      ctx.save();
      ctx.globalAlpha = fondu;
      ctx.translate(p.x, p.y);
      ctx.rotate(p.angle);
      ctx.fillStyle = p.couleur;
      if (p.rond) {
        ctx.beginPath();
        ctx.arc(0, 0, p.taille / 2.5, 0, Math.PI * 2);
        ctx.fill();
      } else {
        ctx.fillRect(-p.taille / 2, -p.taille / 4, p.taille, p.taille / 2);
      }
      ctx.restore();
    }
    if (t - debut < duree) requestAnimationFrame(image);
    else toile.remove();
  };
  requestAnimationFrame(image);
}

type Vue = "question" | "sur" | "merci";

/**
 * Proposer, une fois, de soutenir le projet par une étoile GitHub — après la
 * première vraie réponse, et seulement si GitHub est déjà connecté sur la
 * machine. Rien ne part sans un clic ; la réponse est retenue, on ne
 * redemande jamais.
 */
export function SupportDialog() {
  const [etat, setEtat] = useState<EtatSoutien | null>(null);
  const [vue, setVue] = useState<Vue>("question");
  const [occupe, setOccupe] = useState(false);
  const [merci, setMerci] = useState("");
  const demande = useRef(false);

  useEffect(() => {
    // Dans l'aperçu navigateur, seulement sur demande (`?soutien=1`) : il
    // n'y a pas de vrai compte GitHub derrière.
    if (coreMode !== "tauri" && !new URLSearchParams(window.location.search).has("soutien")) {
      return;
    }
    const surReponse = () => {
      if (demande.current) return;
      demande.current = true;
      window.setTimeout(() => {
        core
          .soutienEtat()
          .then((e) => {
            if (e.proposer) setEtat(e);
          })
          .catch(() => {});
      }, 1500);
    };
    window.addEventListener(REPONSE_FINIE_EVENT, surReponse);
    return () => window.removeEventListener(REPONSE_FINIE_EVENT, surReponse);
  }, []);

  if (!etat) return null;
  const morphs = Math.max(0, etat.depots.length - 1);

  async function repondre(choix: "oui" | "tout" | "non") {
    if (choix === "non") {
      await core.soutienRepondre("non").catch(() => {});
      setEtat(null);
      return;
    }
    setOccupe(true);
    if (choix === "tout") confettis(220, 4);
    else confettis(90, 1);
    try {
      const r = await core.soutienRepondre(choix);
      setMerci(
        r.etoiles > 0
          ? `Merci énormément ! ${r.etoiles} étoile${r.etoiles > 1 ? "s" : ""} posée${r.etoiles > 1 ? "s" : ""}${r.echecs ? ` (${r.echecs} n'ont pas pu l'être)` : ""}.`
          : "Merci ! Les étoiles n'ont pas pu être posées — GitHub n'a pas répondu.",
      );
    } catch (e) {
      setMerci(`Merci ! Les étoiles n'ont pas pu être posées : ${String(e)}`);
    } finally {
      setOccupe(false);
      setVue("merci");
    }
  }

  return createPortal(
    <div className="locaryn-maj-layer">
      <div className="locaryn-maj-scrim" aria-hidden="true" />
      <div
        className="locaryn-maj locaryn-soutien"
        role="dialog"
        aria-modal="true"
        aria-label="Soutenir Locaryn"
      >
        {vue === "question" && (
          <>
            <div className="locaryn-soutien-etoile" aria-hidden="true">
              ⭐
            </div>
            <h2>Locaryn vous plaît ?</h2>
            <p className="locaryn-maj-texte">
              Locaryn est développé bénévolement. Une étoile sur GitHub — depuis votre compte{" "}
              <strong>@{etat.login}</strong> — aide le projet à se faire connaître et montre qu'il
              est utile.
            </p>
            <p className="locaryn-soutien-detail">
              « Oui » met une étoile à Locaryn
              {morphs > 0
                ? ` et aux ${morphs} morph${morphs > 1 ? "s" : ""} que vous utilisez`
                : ""}
              . « Je soutiens à fond » en met une à tous les dépôts du projet Locaryn.
            </p>
            <div className="locaryn-soutien-actions">
              <button
                type="button"
                className="locaryn-soutien-non"
                disabled={occupe}
                onClick={() => {
                  // Le premier « Non » est déjà une réponse : retenue tout de
                  // suite, même si la fenêtre se ferme avant la seconde
                  // question. « Bon, d'accord » la remplace.
                  void core.soutienRepondre("non").catch(() => {});
                  setVue("sur");
                }}
              >
                Non
              </button>
              <button
                type="button"
                className="locaryn-btn locaryn-soutien-oui"
                disabled={occupe}
                onClick={() => void repondre("oui")}
              >
                Oui ⭐
              </button>
              <button
                type="button"
                className="locaryn-soutien-fond"
                disabled={occupe}
                onClick={() => void repondre("tout")}
              >
                JE SOUTIENS À FOND 🚀
              </button>
            </div>
          </>
        )}
        {vue === "sur" && (
          <>
            <div className="locaryn-soutien-etoile" aria-hidden="true">
              🥺
            </div>
            <h2>Vous êtes sûr ?</h2>
            <p className="locaryn-maj-texte">
              Je maintiens Locaryn bénévolement, sur mon temps libre. Savoir que le projet plaît aux
              gens compte énormément pour continuer à l'améliorer — une étoile, c'est un clic, et
              c'est gratuit.
            </p>
            <div className="locaryn-soutien-actions">
              <button
                type="button"
                className="locaryn-soutien-non"
                disabled={occupe}
                onClick={() => void repondre("non")}
              >
                Non, merci
              </button>
              <button
                type="button"
                className="locaryn-btn locaryn-soutien-oui"
                disabled={occupe}
                onClick={() => void repondre("oui")}
              >
                Bon, d'accord ⭐
              </button>
            </div>
          </>
        )}
        {vue === "merci" && (
          <>
            <div className="locaryn-soutien-etoile" aria-hidden="true">
              💚
            </div>
            <h2>Merci !</h2>
            <p className="locaryn-maj-texte">{merci}</p>
            <div className="locaryn-soutien-actions">
              <button
                type="button"
                className="locaryn-btn locaryn-soutien-oui"
                onClick={() => setEtat(null)}
              >
                Fermer
              </button>
            </div>
          </>
        )}
      </div>
    </div>,
    document.body,
  );
}
