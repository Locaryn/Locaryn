import { core, isTauri } from "./core";
import { getWorkspaceConversation, openTab, openTabFor, ouvrirFichier } from "./workspace";

/**
 * Une page HTML de la conversation s'ouvre dans le navigateur intégré de sa
 * conversation, servie avec son dossier (protocole `apercu`) : son CSS, ses
 * scripts et ses images relatifs se chargent comme sur un vrai site.
 */

export function estPageHtml(chemin: string): boolean {
  return /\.html?$/i.test(chemin);
}

/** Ce qu'il faut pour retrouver le fichier d'une page affichée. */
type Apercu = {
  /** L'adresse du dossier servi, barre finale comprise. */
  prefixe: string;
  /** Le dossier de la page, relatif à celui de la conversation (`""` à la racine). */
  dossier: string;
};

/** Les pages ouvertes, par conversation. */
const apercus = new Map<string, Apercu>();

function dossierParent(chemin: string): string {
  const i = chemin.lastIndexOf("/");
  return i < 0 ? "" : chemin.slice(0, i);
}

/**
 * Ouvrir la page `chemin` dans le navigateur de la conversation. Hors de
 * l'application de bureau, elle s'ouvre dans l'onglet Fichiers.
 */
export async function ouvrirApercu(sessionId: string | null, chemin: string): Promise<void> {
  const session = sessionId ?? getWorkspaceConversation();
  if (!isTauri || session === "") {
    ouvrirFichier(chemin);
    return;
  }
  const url = await core.apercuPage(session, chemin);
  apercus.set(session, {
    prefixe: url.slice(0, url.lastIndexOf("/") + 1),
    dossier: dossierParent(chemin),
  });
  await ouvrirAdresse(session, url);
}

/** Afficher `url` dans le navigateur de la conversation `session`. */
export async function ouvrirAdresse(session: string, url: string): Promise<void> {
  openTabFor(session, "browser");
  await core.browserNavigate(session, url);
}

/**
 * Ouvrir un lien ou une recherche dans le navigateur intégré de la
 * conversation affichée ; hors de l'application de bureau, dans un onglet.
 */
export function ouvrirLien(cible: string): void {
  if (!isTauri) {
    window.open(cible, "_blank", "noopener,noreferrer");
    return;
  }
  openTab("browser");
  const conversation = getWorkspaceConversation();
  void core
    .browserNavigate(conversation === "" ? null : conversation, cible)
    .catch((e: unknown) => console.warn("navigateur intégré indisponible :", e));
}

/**
 * Un bloc HTML ou SVG d'une réponse, rendu comme une page dans le navigateur
 * de la conversation affichée.
 */
export async function ouvrirExtrait(code: string, svg: boolean): Promise<void> {
  const url = await core.apercuExtrait(code, svg);
  await ouvrirAdresse(getWorkspaceConversation(), url);
}

/**
 * Le fichier de la page affichée à `url` dans le navigateur de la
 * conversation, s'il vient de son dossier ; `null` pour une page web.
 */
export function fichierDeLaPage(sessionId: string | null, url: string): string | null {
  const apercu = apercus.get(sessionId ?? getWorkspaceConversation());
  if (!apercu || !url.startsWith(apercu.prefixe)) return null;
  const reste = url.slice(apercu.prefixe.length).split(/[?#]/)[0] ?? "";
  let relatif: string;
  try {
    relatif = decodeURIComponent(reste);
  } catch (e) {
    console.warn("adresse d'aperçu illisible :", e);
    return null;
  }
  if (relatif === "" || relatif.endsWith("/")) relatif += "index.html";
  return apercu.dossier ? `${apercu.dossier}/${relatif}` : relatif;
}
