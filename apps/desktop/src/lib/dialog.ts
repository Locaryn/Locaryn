import { open, save } from "@tauri-apps/plugin-dialog";

// ── Les questions de l'application, dessinées par elle ──────────────────────
//
// `confirmer`, `informer` et `demanderTexte` posent une demande dans une file ;
// `AppDialogs` (monté à la racine) l'affiche avec les cartes et boutons de
// l'interface, et la réponse résout la promesse. Aucune boîte du système.

export type DemandeDialogue = {
  id: number;
  titre: string;
  message: string;
} & (
  | { genre: "confirmation"; repondre: (v: boolean) => void }
  | { genre: "message"; repondre: (v: boolean) => void }
  | { genre: "saisie"; defaut: string; repondre: (v: string | null) => void }
);

let file: DemandeDialogue[] = [];
let prochain = 0;
const abonnes = new Set<() => void>();

function prevenir() {
  for (const a of abonnes) a();
}

export function suivreDialogue(rappel: () => void): () => void {
  abonnes.add(rappel);
  return () => abonnes.delete(rappel);
}

/** La question à l'écran : la plus ancienne de la file. */
export function dialogueCourant(): DemandeDialogue | null {
  return file[0] ?? null;
}

export function repondreDialogue(demande: DemandeDialogue, valeur: boolean | string | null) {
  file = file.filter((d) => d.id !== demande.id);
  if (demande.genre === "saisie") demande.repondre(typeof valeur === "string" ? valeur : null);
  else demande.repondre(valeur === true);
  prevenir();
}

/** Demander une confirmation. Fermer la boîte (Échap, clic dehors) vaut
 *  « non » : mieux vaut ne rien supprimer. */
export function confirmer(message: string, titre = "Confirmer ?"): Promise<boolean> {
  return new Promise((repondre) => {
    file = [...file, { id: ++prochain, genre: "confirmation", titre, message, repondre }];
    prevenir();
  });
}

/** Dire quelque chose que la personne doit avoir lu (un échec, le plus souvent). */
export function informer(message: string, titre = "Locaryn"): Promise<void> {
  return new Promise((fini) => {
    file = [...file, { id: ++prochain, genre: "message", titre, message, repondre: () => fini() }];
    prevenir();
  });
}

/** Demander un texte court. `null` : la personne a renoncé. */
export function demanderTexte(titre: string, defaut = ""): Promise<string | null> {
  return new Promise((repondre) => {
    file = [...file, { id: ++prochain, genre: "saisie", titre, message: "", defaut, repondre }];
    prevenir();
  });
}

/** Open the native folder picker. Falls back to a prompt when not running
 *  inside Tauri (e.g. browser dev). */
export async function pickFolder(): Promise<string | null> {
  try {
    return await open({ directory: true, multiple: false });
  } catch {
    return demanderTexte("Chemin du dossier projet");
  }
}

/** Native picker for a single image. Falls back to a prompt outside Tauri. */
export async function pickImageFile(): Promise<string | null> {
  try {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "bmp"] }],
    });
    return typeof picked === "string" ? picked : null;
  } catch {
    return demanderTexte("Chemin de l'image");
  }
}

/** Native picker restricted to given extensions. */
export async function pickAnyFile(label: string, extensions: string[]): Promise<string | null> {
  try {
    const picked = await open({ multiple: false, filters: [{ name: label, extensions }] });
    return typeof picked === "string" ? picked : null;
  } catch {
    return demanderTexte(`Chemin du fichier (${label})`);
  }
}

/** Native Save As picker, with a browser fallback for the UI preview. */
export async function pickSaveFile(
  defaultPath: string,
  extensions: string[],
  label = "Fichier",
): Promise<string | null> {
  try {
    return await save({
      defaultPath,
      filters: [{ name: label, extensions }],
    });
  } catch {
    return demanderTexte(`Enregistrer ${label.toLowerCase()} sous`, defaultPath ?? "");
  }
}
