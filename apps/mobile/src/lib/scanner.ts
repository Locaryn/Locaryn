import { coreMode } from "./core";

/**
 * The in-app camera scanner.
 *
 * It exists because the camera-app route is not guaranteed. Android hands a
 * `locaryn://` link to us when the camera or the browser offers to open it, and
 * many do — but plenty of camera apps only surface `http(s)` links and stay
 * silent on a custom scheme. Telling someone "scan it with your camera" and
 * having nothing happen is worse than one extra tap, so this is the path that
 * always works.
 */
export function isScannerAvailable(): boolean {
  // In a browser there is no camera plugin, but the flow after a scan still
  // needs to be workable — that is where the layout is built.
  return true;
}

/** Returns the scanned text, or null if the user backed out. */
export async function scan(): Promise<string | null> {
  if (coreMode !== "tauri") {
    // Stands in for a code, so the screens after it can be developed in a
    // browser. The verification it feeds is in Rust and is tested there.
    return "locaryn://travel?demo=1";
  }
  const mod = await import("@tauri-apps/plugin-barcode-scanner");

  // Sans cette demande, le scanner s'ouvrait et se refermait dans la même
  // milliseconde sur un téléphone qui n'avait jamais accordé l'appareil photo :
  // aucune permission demandée, aucun message, rien à l'écran. C'était le seul
  // chemin d'entrée de l'application.
  let state = await mod.checkPermissions();
  if (state !== "granted") {
    state = await mod.requestPermissions();
  }
  if (state !== "granted") {
    throw new Error(
      "Locaryn n'a pas accès à l'appareil photo. Autorisez-le dans les réglages " +
        "d'Android, ou tapez l'adresse du serveur.",
    );
  }

  // The camera preview is drawn by the system *behind* the webview, so the
  // page has to become transparent or there is nothing to aim with.
  document.body.classList.add("lo-scanning");
  try {
    const result = await mod.scan({ windowed: true, formats: [mod.Format.QRCode] });
    return result?.content ?? null;
  } catch {
    // La permission est accordée : ce qui reste, c'est le retour arrière. La
    // personne sait ce qu'elle vient de faire, inutile de le lui dire.
    return null;
  } finally {
    document.body.classList.remove("lo-scanning");
    await mod.cancel().catch(() => {});
  }
}
