// La progression d'installation d'une extension, rendue visible.
//
// L'hôte relaie les octets reçus pendant le téléchargement du paquet ; ici ils
// deviennent une tâche du centre de tâches, que la bulle en bas à droite
// affiche. Toutes les voies d'installation y passent : réglages, accueil, et
// l'installation demandée par le modèle dans une conversation.

import { taskCenter } from "./taskCenter";

export const INSTALL_LABEL_PREFIX = "Installation : ";

export interface InstallProgressEvent {
  source: string;
  phase: "download" | "done" | "error";
  done: number;
  total: number | null;
}

/** `Propriétaire/depot-nom#v1.2.0` → `depot nom`, lisible pour tout le monde. */
export function extensionLabel(source: string): string {
  const sansRef = source.split(/[#@]/)[0] ?? source;
  const dernier = sansRef.replace(/\/+$/, "").split("/").pop() ?? sansRef;
  return (
    dernier
      .replace(/\.git$/, "")
      .replace(/[-_]+/g, " ")
      .trim() || source
  );
}

const enCours = new Map<string, string>();

/** Un paquet se compresse mal à l'avance : sans total annoncé, la barre
 *  avance vers 90 % au fil des mégaoctets sans jamais prétendre finir. */
function pourcentage(done: number, total: number | null): number {
  if (total && total > 0) return Math.min(99, Math.round((done / total) * 100));
  const mo = done / (1024 * 1024);
  return Math.min(90, Math.round(90 * (1 - Math.exp(-mo / 12))));
}

export function reportInstallProgress(ev: InstallProgressEvent): void {
  let id = enCours.get(ev.source);
  if (!id) {
    if (ev.phase !== "download") return;
    id = taskCenter.add({
      type: "download",
      label: `${INSTALL_LABEL_PREFIX}${extensionLabel(ev.source)}`,
      progress: 0,
    });
    enCours.set(ev.source, id);
  }
  if (ev.phase === "download") {
    taskCenter.update(id, { progress: pourcentage(ev.done, ev.total) });
    return;
  }
  enCours.delete(ev.source);
  if (ev.phase === "done") taskCenter.done(id);
  else taskCenter.fail(id, "L'installation n'a pas abouti.");
}

/** Écouter l'hôte. Rend de quoi se désabonner. */
export async function listenInstallProgress(): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<InstallProgressEvent>("locaryn://extension-install", (e) =>
    reportInstallProgress(e.payload),
  );
}
