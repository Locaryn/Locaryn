/**
 * Les notifications du système, pour ce qui se termine hors de l'écran.
 *
 * Le centre de notifications de l'application ne sert qu'à qui la regarde. Or
 * on lance un téléchargement de plusieurs gigaoctets et on va faire autre
 * chose : la fenêtre est réduite ou fermée, et rien ne dit que c'est fini. Le
 * système, lui, sait afficher une bannière et jouer un son par-dessus ce que
 * la personne est en train de faire.
 *
 * Deux règles. On ne prévient pas quelqu'un qui regarde déjà : la fenêtre au
 * premier plan montre la même information, et doubler chaque fin de tâche
 * d'une bannière deviendrait vite insupportable. Et on ne prévient jamais
 * hors de l'application de bureau — le navigateur n'a pas cette permission,
 * et la demander pour ça serait un mauvais échange.
 */

type ModuleNotification = {
  isPermissionGranted: () => Promise<boolean>;
  requestPermission: () => Promise<string>;
  sendNotification: (options: { title: string; body?: string }) => void;
};

/** Chargé à la demande : le paquet n'existe que dans la version de bureau. */
let module: Promise<ModuleNotification | null> | null = null;

/** Résultat de la demande d'autorisation, posée une seule fois par session. */
let autorise: Promise<boolean> | null = null;

function dansTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function charger(): Promise<ModuleNotification | null> {
  if (!dansTauri()) return null;
  if (!module) {
    module = import("@tauri-apps/plugin-notification")
      .then((m) => m as unknown as ModuleNotification)
      .catch(() => null);
  }
  return module;
}

async function permission(api: ModuleNotification): Promise<boolean> {
  if (!autorise) {
    autorise = (async () => {
      try {
        if (await api.isPermissionGranted()) return true;
        return (await api.requestPermission()) === "granted";
      } catch {
        // Un système qui refuse d'être interrogé n'est pas une panne de
        // l'application : on se tait, le centre de notifications reste là.
        return false;
      }
    })();
  }
  return autorise;
}

/**
 * Poser une bannière du système, si personne ne regarde déjà la fenêtre.
 *
 * Ne rejette jamais : une notification qui n'arrive pas ne doit pas faire
 * échouer le travail qu'elle annonçait.
 */
export async function notifierSysteme(titre: string, corps?: string): Promise<void> {
  try {
    if (typeof document !== "undefined" && document.hasFocus()) return;
    const api = await charger();
    if (!api) return;
    if (!(await permission(api))) return;
    api.sendNotification({ title: titre, body: corps });
  } catch (error) {
    console.warn("[Locaryn] notification système impossible :", error);
  }
}
