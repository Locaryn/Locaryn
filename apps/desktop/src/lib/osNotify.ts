/**
 * Les notifications du système, pour ce qui se termine hors de l'écran.
 *
 * Le centre de notifications de l'application ne sert qu'à qui la regarde. Or
 * on lance un téléchargement de plusieurs gigaoctets et on va faire autre
 * chose : la fenêtre est réduite ou fermée, et rien ne dit que c'est fini. Le
 * système, lui, sait afficher une bannière et jouer un son par-dessus ce que
 * la personne est en train de faire.
 *
 * Trois règles, et elles ont toutes une raison.
 *
 * **On ne prévient pas quelqu'un qui regarde déjà** : la fenêtre au premier
 * plan montre la même information, et doubler chaque fin de tâche d'une
 * bannière deviendrait vite insupportable. La règle se désactive pour qui
 * travaille sur plusieurs écrans.
 *
 * **Une tâche finie en trois secondes ne mérite pas de bannière.** Seule celle
 * qu'on a eu le temps d'oublier la mérite, d'où le seuil de durée.
 *
 * **Chaque bannière appartient à un groupe**, et un groupe se coupe. Sans
 * groupe, la seule alternative aurait été de tout couper.
 *
 * On ne prévient jamais hors de l'application de bureau — le navigateur n'a pas
 * cette permission, et la demander pour ça serait un mauvais échange.
 */

import { type NotificationGroup, type NotificationPrefs, core } from "./core";

type ModuleNotification = {
  isPermissionGranted: () => Promise<boolean>;
  requestPermission: () => Promise<string>;
  sendNotification: (options: { title: string; body?: string }) => void;
};

/** Chargé à la demande : le paquet n'existe que dans la version de bureau. */
let module: Promise<ModuleNotification | null> | null = null;

/** Résultat de la demande d'autorisation, posée une seule fois par session. */
let autorise: Promise<boolean> | null = null;

/**
 * Les préférences, gardées le temps d'une session.
 *
 * Elles sont lues à chaque notification sans ce cache, soit un aller-retour IPC
 * pour chaque fin de tâche. L'écran des réglages appelle [`oublierPreferences`]
 * après un enregistrement, si bien qu'un changement s'applique tout de suite.
 */
let preferences: Promise<NotificationPrefs> | null = null;

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

async function lirePreferences(): Promise<NotificationPrefs> {
  if (!preferences) {
    preferences = core.notificationPrefs().catch(() => ({
      // Préférences illisibles : on notifie plutôt que de se taire. Une
      // bannière de trop se remarque et se règle ; une bannière manquante
      // laisse croire que l'application ne fait rien.
      enabled: true,
      approvals: true,
      downloads: true,
      long_tasks: true,
      model_lifecycle: true,
      security: true,
      maintenance: true,
      only_when_hidden: true,
      min_duration_seconds: 20,
      taskbar_progress: true,
    }));
  }
  return preferences;
}

/** À appeler après un enregistrement, pour que le changement porte aussitôt. */
export function oublierPreferences(): void {
  preferences = null;
}

/** Ce qui décide si une bannière part, hors permission du système. */
export async function notificationAutorisee(
  groupe: NotificationGroup,
  dureeMs?: number,
): Promise<boolean> {
  const p = await lirePreferences();
  if (!p.enabled || !p[groupe]) return false;
  if (p.only_when_hidden && typeof document !== "undefined" && document.hasFocus()) {
    return false;
  }
  // Le seuil ne s'applique qu'à ce qui a une durée. Une approbation qui attend
  // n'a pas de durée écoulée : elle doit partir tout de suite, c'est justement
  // ce qui bloque le travail.
  if (dureeMs !== undefined && dureeMs < p.min_duration_seconds * 1000) return false;
  return true;
}

/**
 * Poser une bannière du système.
 *
 * Ne rejette jamais : une notification qui n'arrive pas ne doit pas faire
 * échouer le travail qu'elle annonçait.
 *
 * `dureeMs` est la durée de la tâche annoncée, quand elle en a une : c'est elle
 * qui décide si la bannière valait la peine.
 */
export async function notifierSysteme(
  titre: string,
  corps?: string,
  groupe: NotificationGroup = "long_tasks",
  dureeMs?: number,
): Promise<void> {
  try {
    if (!dansTauri()) return;
    if (!(await notificationAutorisee(groupe, dureeMs))) return;
    const api = await charger();
    if (!api) return;
    if (!(await permission(api))) return;
    api.sendNotification({ title: titre, body: corps });
  } catch (error) {
    console.warn("[Locaryn] notification système impossible :", error);
  }
}
