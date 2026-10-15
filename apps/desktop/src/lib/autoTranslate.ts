// Traduire toute l'interface affichée, pas seulement les textes passés à `t()`.
//
// L'application est écrite en français, et `t()` n'était appelé qu'à une
// poignée d'endroits : passer en anglais ne changeait presque rien. Plutôt que
// de réécrire des centaines de composants, ce module traduit ce qui est
// réellement à l'écran : chaque texte et chaque attribut lisible (title,
// placeholder, aria-label, alt) est cherché dans le dictionnaire, et remplacé
// s'il y figure. Un `MutationObserver` suit ce que React ajoute ou change.
//
// Ce qui n'est jamais traduit :
// - les messages de la conversation (ce que la personne et le modèle écrivent) ;
// - le code, les champs de saisie, ce qui porte `data-no-translate` ;
// - les noms de modèles, de marques et les termes techniques — ils ne sont pas
//   dans le dictionnaire, donc ils passent tels quels.
//
// Revenir au français restaure les textes d'origine.

import { EN_EXACT, EN_PATTERNS } from "./i18nEn";

type Lang = "fr" | "en";

const ATTRS = ["title", "placeholder", "aria-label", "alt"] as const;

/** Les zones dont le contenu appartient à la personne ou au modèle. */
const EXCLUS =
  ".locaryn-msg-row, .locaryn-msg, .locaryn-msg-text, .md-code, pre, code, textarea, input, [contenteditable='true'], [data-no-translate], .xterm, .monaco-editor, script, style";

/** Les gabarits compilés : « Téléchargement de {} » → /^Téléchargement de (.+?)$/. */
const MOTIFS: { re: RegExp; en: string }[] = Object.entries(EN_PATTERNS).map(([fr, en]) => {
  const parts = fr.split("{}").map((p) => p.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  return { re: new RegExp(`^${parts.join("([\\s\\S]+?)")}$`), en };
});

/** Les textes déjà traduits : le texte affiché → l'original. */
const origineTexte = new WeakMap<Text, { fr: string; en: string }>();
const origineAttr = new WeakMap<Element, Map<string, { fr: string; en: string }>>();
/** Ce qu'il faudra restaurer en revenant au français. */
let touches: WeakRef<Node>[] = [];

function normaliser(s: string): string {
  return s.replace(/\s+/g, " ").trim();
}

/** La traduction d'un texte, ou `null` s'il n'en a pas. */
export function traduire(brut: string): string | null {
  const cle = normaliser(brut);
  if (cle.length < 2 || !/[A-Za-zÀ-ÿ]/.test(cle)) return null;
  let en = EN_EXACT[cle];
  if (en === undefined && cle.length < 600) {
    for (const m of MOTIFS) {
      const r = m.re.exec(cle);
      if (!r) continue;
      let i = 1;
      en = m.en.replace(/\{\}/g, () => {
        const val = r[i++] ?? "";
        // Une valeur peut être elle-même un texte connu (« Chargement de … »).
        return EN_EXACT[normaliser(val)] ?? val;
      });
      break;
    }
  }
  if (en === undefined) return null;
  // Garder les blancs autour : ils séparent souvent deux morceaux de JSX.
  const avant = /^\s*/.exec(brut)?.[0] ?? "";
  const apres = /\s*$/.exec(brut)?.[0] ?? "";
  return avant + en + apres;
}

function exclu(el: Element | null): boolean {
  return !el || el.closest(EXCLUS) !== null;
}

function traduireTexte(n: Text) {
  const actuel = n.data;
  const deja = origineTexte.get(n);
  // Notre propre écriture qui revient par l'observateur : rien à faire.
  if (deja && deja.en === actuel) return;
  if (exclu(n.parentElement)) return;
  const en = traduire(actuel);
  if (en === null || en === actuel) return;
  origineTexte.set(n, { fr: actuel, en });
  touches.push(new WeakRef(n));
  n.data = en;
}

function traduireAttributs(el: Element) {
  if (el.closest("[data-no-translate]")) return;
  for (const a of ATTRS) {
    const v = el.getAttribute(a);
    if (!v) continue;
    let carte = origineAttr.get(el);
    const deja = carte?.get(a);
    if (deja && deja.en === v) continue;
    const en = traduire(v);
    if (en === null || en === v) continue;
    if (!carte) {
      carte = new Map();
      origineAttr.set(el, carte);
      touches.push(new WeakRef(el));
    }
    carte.set(a, { fr: v, en });
    el.setAttribute(a, en);
  }
}

function parcourir(racine: Node) {
  if (racine.nodeType === Node.TEXT_NODE) {
    traduireTexte(racine as Text);
    return;
  }
  if (racine.nodeType !== Node.ELEMENT_NODE) return;
  const el = racine as Element;
  if (el.matches?.(EXCLUS) && !el.matches("input, textarea")) return;
  traduireAttributs(el);
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT, {
    acceptNode(n) {
      if (n.nodeType === Node.ELEMENT_NODE) {
        const e = n as Element;
        // Les champs gardent leur placeholder traduisible, pas leur contenu.
        if (e.matches("input, textarea")) {
          traduireAttributs(e);
          return NodeFilter.FILTER_REJECT;
        }
        if (e.matches(EXCLUS)) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      }
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  let n = walker.nextNode();
  while (n) {
    if (n.nodeType === Node.TEXT_NODE) traduireTexte(n as Text);
    else traduireAttributs(n as Element);
    n = walker.nextNode();
  }
}

let observateur: MutationObserver | null = null;

function demarrer() {
  if (observateur || typeof document === "undefined") return;
  parcourir(document.body);
  observateur = new MutationObserver((mutations) => {
    for (const m of mutations) {
      if (m.type === "characterData") traduireTexte(m.target as Text);
      else if (m.type === "attributes") traduireAttributs(m.target as Element);
      else for (const n of m.addedNodes) parcourir(n);
    }
  });
  observateur.observe(document.body, {
    subtree: true,
    childList: true,
    characterData: true,
    attributes: true,
    attributeFilter: [...ATTRS],
  });
}

function arreter() {
  observateur?.disconnect();
  observateur = null;
  // Rendre les textes d'origine.
  for (const ref of touches) {
    const n = ref.deref();
    if (!n) continue;
    if (n.nodeType === Node.TEXT_NODE) {
      const o = origineTexte.get(n as Text);
      if (o && (n as Text).data === o.en) (n as Text).data = o.fr;
      origineTexte.delete(n as Text);
    } else {
      const el = n as Element;
      const carte = origineAttr.get(el);
      carte?.forEach((o, a) => {
        if (el.getAttribute(a) === o.en) el.setAttribute(a, o.fr);
      });
      origineAttr.delete(el);
    }
  }
  touches = [];
}

/** Appliquer la langue choisie à toute l'interface. */
export function appliquerLangue(lang: Lang) {
  if (lang === "en") demarrer();
  else arreter();
}
