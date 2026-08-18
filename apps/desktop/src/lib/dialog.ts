import { open, save } from "@tauri-apps/plugin-dialog";

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
): Promise<string | null> {
  try {
    return await save({
      defaultPath,
      filters: [{ name: "Audio", extensions }],
    });
  } catch {
    return window.prompt("Enregistrer la note vocale sous :", defaultPath);
  }
}
