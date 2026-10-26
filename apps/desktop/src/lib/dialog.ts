import { confirm, open, save } from "@tauri-apps/plugin-dialog";

/** Demander une confirmation, par la boîte native.
 *
 *  Sous Tauri, `window.confirm` ne rend pas un booléen : écrit
 *  `if (!window.confirm(…)) return`, il ne retenait jamais rien, et une
 *  désinstallation ou une suppression partait sans qu'aucune question ne
 *  s'affiche. Hors de Tauri (aperçu web), la boîte du navigateur ; et si elle
 *  rend autre chose qu'un booléen, la réponse est « non » : mieux vaut ne
 *  rien supprimer. */
export async function confirmer(message: string, titre = "Locaryn"): Promise<boolean> {
  try {
    return await confirm(message, {
      title: titre,
      kind: "warning",
      okLabel: "Confirmer",
      cancelLabel: "Annuler",
    });
  } catch (e) {
    console.warn("[dialog] boîte de confirmation native indisponible :", e);
    return window.confirm(message) === true;
  }
}

/** Open the native folder picker. Falls back to a prompt when not running
 *  inside Tauri (e.g. browser dev). */
export async function pickFolder(): Promise<string | null> {
  try {
    return await open({ directory: true, multiple: false });
  } catch {
    return window.prompt("Chemin du dossier projet:");
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
    return window.prompt("Chemin de l'image :");
  }
}

/** Native picker restricted to given extensions. */
export async function pickAnyFile(label: string, extensions: string[]): Promise<string | null> {
  try {
    const picked = await open({ multiple: false, filters: [{ name: label, extensions }] });
    return typeof picked === "string" ? picked : null;
  } catch {
    return window.prompt(`Chemin du fichier (${label}) :`);
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
    return window.prompt(`Enregistrer ${label.toLowerCase()} sous :`, defaultPath);
  }
}
