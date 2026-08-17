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

/**
 * Refermer la caméra depuis l'extérieur — le bouton « Annuler » de l'écran.
 *
 * Le greffon n'expose pas d'autre sortie que l'annulation : `scan()` rendra
 * `null`, et l'appelant traitera cela comme un renoncement.
 */
export async function annulerScan(): Promise<void> {
  if (coreMode !== "tauri") return;
  const mod = await import("@tauri-apps/plugin-barcode-scanner");
  await mod.cancel().catch(() => {});
}

/**
 * Lire un QR code. Rend son contenu, ou `null` si la personne a renoncé.
 *
 * `pendant` est appelé une fois la caméra ouverte et une fois refermée : c'est
 * ce qui permet à l'écran d'afficher le cadre de visée. Sans lui, la page
 * devenait transparente pour laisser voir la caméra et plus rien n'était
 * dessiné — un écran vide, sans repère et sans moyen d'en sortir autrement
 * que par le bouton retour.
 */
export async function scan(pendant?: (ouvert: boolean) => void): Promise<string | null> {
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

  // L'aperçu de la caméra est dessiné par le système *derrière* la vue web :
  // la page doit devenir transparente, sinon on ne voit rien. Le fond de la
  // vue web elle-même est rendu transparent côté Android — le CSS seul n'y
  // suffit pas, la vue peint son propre fond opaque par-dessus la caméra.
  document.body.classList.add("lo-scanning");
  pendant?.(true);
  try {
    // Sur un appareil sans le module de lecture de codes déjà en cache — vu
    // en test, sur un émulateur à la connexion lente — la caméra reste
    // ouverte indéfiniment pendant que le greffon retente son chargement en
    // boucle, et ces tentatives peuvent occuper le processeur au point que
    // même le bouton Annuler et le retour matériel cessent de répondre. Un
    // délai borné rend la main à la personne dans tous les cas plutôt que de
    // la laisser bloquée sans échappatoire.
    const resultat = await Promise.race([
      mod.scan({ windowed: true, formats: [mod.Format.QRCode] }),
      delaiEcoule(),
    ]);
    if (resultat === EXPIRE) {
      throw new Error("La caméra n'a pas répondu. Réessayez, ou tapez l'adresse du serveur.");
    }
    return resultat?.content ?? null;
  } catch (e) {
    if (e instanceof Error && e.message.includes("n'a pas répondu")) throw e;
    // La permission est accordée : ce qui reste, c'est le retour arrière. La
    // personne sait ce qu'elle vient de faire, inutile de le lui dire.
    return null;
  } finally {
    document.body.classList.remove("lo-scanning");
    pendant?.(false);
    await mod.cancel().catch(() => {});
  }
}

const EXPIRE = Symbol("scan expiré");

function delaiEcoule(): Promise<typeof EXPIRE> {
  return new Promise((resolve) => window.setTimeout(() => resolve(EXPIRE), 20_000));
}
