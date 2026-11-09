import { estPageHtml, ouvrirAdresse, ouvrirApercu } from "./apercuPage";
import { type TerminalEvent, type WorkspaceEntry, core } from "./core";
import { confirmer } from "./dialog";
import { appendRunLine, finishTerminalRun, startTerminalRun } from "./runPanel";
import { getWorkspaceConversation } from "./workspace";

/**
 * « Exécuter » un fichier de la conversation, selon ce qu'il est :
 * - une page HTML s'affiche dans le navigateur, avec son dossier ;
 * - un projet node (`package.json`) lance son script de développement, puis
 *   affiche l'adresse que le serveur annonce ;
 * - un script (Python, Node, shell, PowerShell…) tourne dans le terminal du
 *   panneau de droite ;
 * - le reste est refusé avec une raison.
 *
 * Toute commande passe d'abord par la confirmation de la personne, comme les
 * blocs de code d'une réponse.
 */

export type GenreExecution = "page" | "projet" | "script";

const WINDOWS = typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");

/** Les scripts du disque et la commande qui les lance (le nom suit). */
const INTERPRETES: Record<string, { libelle: string; commande: string }> = {
  py: { libelle: "Python", commande: WINDOWS ? "python" : "python3" },
  js: { libelle: "Node", commande: "node" },
  mjs: { libelle: "Node", commande: "node" },
  cjs: { libelle: "Node", commande: "node" },
  ts: { libelle: "TypeScript", commande: "npx --yes tsx" },
  sh: { libelle: "Shell", commande: "bash" },
  ps1: {
    libelle: "PowerShell",
    commande: `${WINDOWS ? "powershell" : "pwsh"} -NoProfile -ExecutionPolicy Bypass -File`,
  },
  rb: { libelle: "Ruby", commande: "ruby" },
  php: { libelle: "PHP", commande: "php" },
  ...(WINDOWS
    ? {
        bat: { libelle: "Invite de commandes", commande: "cmd /C" },
        cmd: { libelle: "Invite de commandes", commande: "cmd /C" },
      }
    : {}),
};

/** Les scripts de `package.json` qui démarrent une application, par préférence. */
const SCRIPTS_DE_LANCEMENT = ["dev", "start", "serve", "preview"];

function nomDe(chemin: string): string {
  return chemin.slice(chemin.lastIndexOf("/") + 1);
}

function dossierDe(chemin: string): string {
  const i = chemin.lastIndexOf("/");
  return i < 0 ? "" : chemin.slice(0, i);
}

function extension(chemin: string): string {
  const nom = nomDe(chemin);
  const i = nom.lastIndexOf(".");
  return i < 0 ? "" : nom.slice(i + 1).toLowerCase();
}

/** Ce que « Exécuter » ferait de ce fichier ; `null` s'il ne s'exécute pas. */
export function genreExecution(chemin: string): GenreExecution | null {
  if (estPageHtml(chemin)) return "page";
  if (nomDe(chemin) === "package.json") return "projet";
  if (INTERPRETES[extension(chemin)]) return "script";
  return null;
}

/** Un nom de fichier en argument de commande. */
function argument(nom: string): string {
  return WINDOWS ? `"${nom}"` : `'${nom.replace(/'/g, `'\\''`)}'`;
}

/** Le dossier absolu d'un chemin relatif de la conversation. */
async function dossierAbsolu(session: string, relatif: string): Promise<string> {
  const racine = (await core.sessionWorkspace(session)).replace(/[\\/]+$/, "");
  return relatif ? `${racine}/${relatif}` : racine;
}

const ANSI = new RegExp(`${String.fromCharCode(27)}\\[[0-9;?]*[A-Za-z]`, "g");

/** L'adresse locale qu'un serveur de développement annonce, s'il y en a une. */
export function adresseAnnoncee(ligne: string): string | null {
  const m = /https?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]):\d+[^\s'"]*/.exec(
    ligne.replace(ANSI, ""),
  );
  return m ? m[0].replace("0.0.0.0", "localhost") : null;
}

let compteur = 0;

/**
 * Lancer `commande` dans le terminal du panneau, arrêtable. `surLigne` voit
 * chaque ligne de sortie, débarrassée de ses couleurs.
 */
async function lancer(
  libelle: string,
  commande: string,
  cwd: string,
  surLigne?: (ligne: string) => void,
): Promise<void> {
  compteur += 1;
  const id = `execution-${Date.now().toString(36)}-${compteur}`;
  startTerminalRun(libelle, commande, cwd, () => {
    core.runTerminalStop(id).catch((e: unknown) => appendRunLine(`Arrêt impossible : ${e}`));
  });
  let fini = false;
  const recevoir = (ev: TerminalEvent) => {
    if (ev.type === "line") {
      const ligne = ev.text.replace(ANSI, "");
      appendRunLine(ligne);
      surLigne?.(ligne);
    } else if (ev.type === "exit") {
      fini = true;
      finishTerminalRun(ev.code ?? 0);
    }
  };
  try {
    await core.runTerminal(commande, cwd, recevoir, id);
    if (!fini) finishTerminalRun(0);
  } catch (e) {
    appendRunLine(String(e));
    finishTerminalRun(1);
  }
}

/** Le gestionnaire de paquets d'un projet, d'après son fichier de verrou. */
function gestionnaire(entrees: WorkspaceEntry[]): string {
  const noms = new Set(entrees.map((e) => e.nom));
  if (noms.has("pnpm-lock.yaml")) return "pnpm";
  if (noms.has("yarn.lock")) return "yarn";
  if (noms.has("bun.lockb") || noms.has("bun.lock")) return "bun";
  return "npm";
}

function scriptsDe(texte: string | null, chemin: string): Record<string, string> {
  if (texte === null) throw new Error(`${chemin} est illisible.`);
  let paquet: unknown;
  try {
    paquet = JSON.parse(texte);
  } catch (e) {
    throw new Error(`${chemin} n'est pas un JSON valide : ${e}`);
  }
  const scripts = (paquet as { scripts?: unknown } | null)?.scripts;
  if (typeof scripts !== "object" || scripts === null) return {};
  return Object.fromEntries(
    Object.entries(scripts).filter((e): e is [string, string] => typeof e[1] === "string"),
  );
}

/** Un projet node : son script de développement, puis l'aperçu de son adresse. */
async function lancerProjet(session: string, chemin: string): Promise<void> {
  const dossier = dossierDe(chemin);
  const [paquet, entrees] = await Promise.all([
    core.workspaceRead(session, chemin),
    core.workspaceList(session, dossier),
  ]);
  const scripts = scriptsDe(paquet.contenu, chemin);
  const script = SCRIPTS_DE_LANCEMENT.find((s) => s in scripts);
  if (!script) {
    // Pas de serveur à lancer : une page statique à côté se montre telle quelle.
    const page = entrees.find((e) => !e.dossier && e.nom.toLowerCase() === "index.html");
    if (page) return ouvrirApercu(session, page.chemin);
    throw new Error(
      `${chemin} n'a ni script ${SCRIPTS_DE_LANCEMENT.map((s) => `« ${s} »`).join(", ")}, ni index.html à afficher.`,
    );
  }
  const pm = gestionnaire(entrees);
  const installer = !entrees.some((e) => e.dossier && e.nom === "node_modules");
  const commande = `${installer ? `${pm} install && ` : ""}${pm} run ${script}`;
  const cwd = await dossierAbsolu(session, dossier);
  const ok = await confirmer(
    `Lancer le serveur de développement ?\n\n${commande}\n\ndans ${cwd}${
      installer ? "\n\nLes dépendances seront d'abord installées." : ""
    }`,
  );
  if (!ok) return;
  let ouvert = false;
  // La commande tourne jusqu'à ce qu'on l'arrête : on n'attend pas sa fin.
  void lancer(`Serveur de dev · ${pm} run ${script}`, commande, cwd, (ligne) => {
    const url = ouvert ? null : adresseAnnoncee(ligne);
    if (!url) return;
    ouvert = true;
    ouvrirAdresse(session, url).catch((e: unknown) =>
      appendRunLine(`Aperçu impossible sur ${url} : ${e}`),
    );
  });
}

/** Un script : son interprète, dans son dossier. */
async function lancerScript(session: string, chemin: string): Promise<void> {
  const interprete = INTERPRETES[extension(chemin)];
  if (!interprete) throw new Error(`${nomDe(chemin)} ne s'exécute pas ici.`);
  const commande = `${interprete.commande} ${argument(nomDe(chemin))}`;
  const cwd = await dossierAbsolu(session, dossierDe(chemin));
  const ok = await confirmer(
    `Exécuter ce script (${interprete.libelle}) ?\n\n${commande}\n\ndans ${cwd}`,
  );
  if (!ok) return;
  await lancer(interprete.libelle, commande, cwd);
}

/**
 * Exécuter `chemin`, relatif au dossier de la conversation `sessionId` (la
 * conversation affichée si `null`). Lève une erreur lisible quand le fichier
 * ne s'exécute pas.
 */
export async function executerFichier(sessionId: string | null, chemin: string): Promise<void> {
  const session = sessionId ?? getWorkspaceConversation();
  if (session === "") throw new Error("Ouvrez une conversation pour exécuter ses fichiers.");
  switch (genreExecution(chemin)) {
    case "page":
      return ouvrirApercu(session, chemin);
    case "projet":
      return lancerProjet(session, chemin);
    case "script":
      return lancerScript(session, chemin);
    default:
      throw new Error(
        `Locaryn ne sait pas exécuter « ${nomDe(chemin)} ». Il exécute les pages HTML, les projets node (package.json) et les scripts Python, Node, shell ou PowerShell.`,
      );
  }
}
