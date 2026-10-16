import { useEffect, useRef, useState } from "react";

/**
 * Le texte d'une réponse en cours, dévoilé caractère par caractère.
 *
 * Le modèle envoie ses jetons par paquets irréguliers : afficher chaque paquet
 * tel quel faisait avancer le texte par à-coups. Ici, ce qui est arrivé se
 * dévoile à un rythme régulier, qui accélère quand le retard grandit — on ne
 * traîne jamais plus d'un quart de seconde derrière le modèle.
 *
 * Un message déjà complet (l'historique, une conversation rouverte) s'affiche
 * d'un coup : seul un texte qui s'allonge sous nos yeux est animé.
 */
export function useTexteFluide(cible: string): string {
  const [montre, setMontre] = useState(cible);
  const montreRef = useRef(cible);
  const cibleRef = useRef(cible);
  const anim = useRef<number | null>(null);
  cibleRef.current = cible;

  useEffect(() => {
    const actuel = montreRef.current;
    // Pas un prolongement (message remplacé, édité, raccourci) : on suit.
    if (!cible.startsWith(actuel)) {
      montreRef.current = cible;
      setMontre(cible);
      return;
    }
    if (cible.length === actuel.length || anim.current !== null) return;
    let precedent = performance.now();
    const pas = (maintenant: number) => {
      const dt = Math.min(64, maintenant - precedent);
      precedent = maintenant;
      const but = cibleRef.current;
      const ici = montreRef.current;
      if (!but.startsWith(ici)) {
        montreRef.current = but;
        setMontre(but);
        anim.current = null;
        return;
      }
      const retard = but.length - ici.length;
      if (retard <= 0) {
        anim.current = null;
        return;
      }
      // Au moins ~90 caractères par seconde, et tout le retard rattrapé en
      // ~250 ms : régulier quand le modèle est lent, jamais à la traîne.
      const vitesse = Math.max(90, retard * 4);
      let n = Math.max(1, Math.round((vitesse * dt) / 1000));
      // Ne pas couper une paire de substitution (émoji).
      const code = but.charCodeAt(ici.length + n - 1);
      if (code >= 0xd800 && code <= 0xdbff) n += 1;
      const suivant = but.slice(0, Math.min(but.length, ici.length + n));
      montreRef.current = suivant;
      setMontre(suivant);
      anim.current = requestAnimationFrame(pas);
    };
    anim.current = requestAnimationFrame(pas);
  }, [cible]);

  useEffect(
    () => () => {
      if (anim.current !== null) cancelAnimationFrame(anim.current);
    },
    [],
  );

  return montre;
}

/**
 * Remplacer le contenu de `racine` par `html` en ne touchant que ce qui a
 * changé.
 *
 * Réinjecter tout le HTML à chaque jeton reconstruisait chaque bloc : un bloc
 * de code perdait sa barre (Copier, Exécuter) puis la retrouvait après
 * l'affichage, et la page sautait à chaque jeton. Ici, un bloc identique est
 * gardé tel quel, et un bloc de code qui s'allonge ne voit changer que son
 * code — sa barre reste en place.
 */
export function patcherHtml(racine: HTMLElement, html: string): void {
  const modele = document.createElement("template");
  modele.innerHTML = html;
  const neufs = Array.from(modele.content.childNodes);
  const anciens = Array.from(racine.childNodes);
  neufs.forEach((neuf, i) => {
    const ancien = anciens[i];
    if (!ancien) {
      racine.appendChild(neuf);
      return;
    }
    if (estBlocDeCode(ancien) && estBlocDeCode(neuf) && ancien.dataset.lang === neuf.dataset.lang) {
      const a = ancien.querySelector("code");
      const n = neuf.querySelector("code");
      if (a && n) {
        if (a.innerHTML !== n.innerHTML) a.innerHTML = n.innerHTML;
        return;
      }
    }
    if (!ancien.isEqualNode(neuf)) racine.replaceChild(neuf, ancien);
  });
  for (let i = anciens.length - 1; i >= neufs.length; i--) anciens[i].remove();
}

function estBlocDeCode(n: Node): n is HTMLPreElement {
  return n instanceof HTMLPreElement && n.classList.contains("md-code");
}
