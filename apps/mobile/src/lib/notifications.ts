/**
 * Les notifications du téléphone : réponse prête, autorisation en attente,
 * modèle téléchargé, média produit.
 *
 * Dans l'application, elles passent par le plugin natif de Tauri : l'API
 * `Notification` du navigateur n'existe pas dans la WebView Android — aucune
 * demande d'autorisation ne s'affichait et rien ne partait. L'API du navigateur
 * ne sert plus que dans l'aperçu web.
 */

import {
  sendNotification as envoyerNatif,
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";

const STORAGE_KEY = "locaryn_push_notifications_enabled";
/** La demande Android n'est faite d'office qu'une fois : ensuite, c'est le réglage. */
const DEMANDEE_KEY = "locaryn_notifications_demandees";

const natif = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Etat = NotificationPermission | "unsupported";

/** La permission connue, mise à jour dès que le système répond. */
let etat: Etat = natif
  ? "default"
  : typeof window !== "undefined" && "Notification" in window
    ? Notification.permission
    : "unsupported";

function lire(cle: string): string | null {
  try {
    return localStorage.getItem(cle);
  } catch (e) {
    console.warn("réglage de notification illisible :", e);
    return null;
  }
}

function ecrire(cle: string, valeur: string): void {
  try {
    localStorage.setItem(cle, valeur);
  } catch (e) {
    console.warn("réglage de notification non enregistré :", e);
  }
}

export function getNotificationPermission(): Etat {
  return etat;
}

export function isPushEnabled(): boolean {
  if (etat !== "granted") return false;
  const stored = lire(STORAGE_KEY);
  return stored === null ? true : stored === "true";
}

export function setPushEnabled(enabled: boolean): void {
  ecrire(STORAGE_KEY, enabled ? "true" : "false");
}

/**
 * Au démarrage : relire la permission, et la demander une première fois
 * d'office (la fenêtre Android « Autoriser Locaryn à vous envoyer des
 * notifications ? »). Un refus ne se redemande pas : il se change dans Réglages.
 */
export async function initNotifications(): Promise<void> {
  if (!natif) return;
  try {
    if (await isPermissionGranted()) {
      etat = "granted";
      return;
    }
    if (lire(DEMANDEE_KEY)) {
      etat = "denied";
      return;
    }
    ecrire(DEMANDEE_KEY, "1");
    etat = await requestPermission();
    if (etat === "granted") setPushEnabled(true);
  } catch (e) {
    console.warn("permission de notification illisible :", e);
  }
}

export async function requestNotificationPermission(): Promise<boolean> {
  try {
    if (natif) {
      etat = (await isPermissionGranted()) ? "granted" : await requestPermission();
    } else if (etat !== "unsupported") {
      etat = await Notification.requestPermission();
    } else {
      return false;
    }
  } catch (e) {
    console.warn("Échec de la demande de permission de notification :", e);
    return false;
  }
  const ok = etat === "granted";
  setPushEnabled(ok);
  return ok;
}

export function sendNotification(
  title: string,
  options?: {
    body?: string;
    tag?: string;
  },
): boolean {
  if (!isPushEnabled()) return false;
  try {
    if (natif) {
      envoyerNatif({ title, body: options?.body });
    } else {
      new Notification(title, {
        icon: "/icons/icon-192.png",
        badge: "/icons/icon-192.png",
        ...options,
      });
    }
    return true;
  } catch (e) {
    console.warn("Échec de l'envoi de la notification :", e);
    return false;
  }
}

/** Notifier qu'une réponse du modèle est prête (quand l'utilisateur est en arrière-plan) */
export function notifyMessageReceived(serverName: string, text: string): void {
  const preview = text.length > 90 ? `${text.slice(0, 87)}…` : text;
  sendNotification(`Réponse de ${serverName}`, {
    body: preview,
    tag: "chat-reply",
  });
}

/** Notifier qu'une génération de média (image, audio) est terminée */
export function notifyMediaComplete(kind: "image" | "audio", name: string): void {
  sendNotification(`Génération terminée (${kind === "image" ? "Image" : "Audio"})`, {
    body: `Le fichier ${name} a été produit avec succès sur le serveur.`,
    tag: "media-gen",
  });
}

/** Notifier qu'un téléchargement de modèle est achevé */
export function notifyModelDownloaded(modelName: string): void {
  sendNotification("Modèle téléchargé", {
    body: `Le modèle « ${modelName} » est prêt à être utilisé sur votre serveur.`,
    tag: "model-pull",
  });
}

/** Notifier qu'une demande d'autorisation est en attente */
export function notifyToolApprovalRequired(toolName: string, risk: string): void {
  sendNotification("Demande d'autorisation requise", {
    body: `L'outil « ${toolName} » (${risk}) attend votre validation pour continuer.`,
    tag: "tool-approval",
  });
}
