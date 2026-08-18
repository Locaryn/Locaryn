/**
 * Gestion des notifications push et alertes système pour l'application mobile Locaryn.
 */

const STORAGE_KEY = "locaryn_push_notifications_enabled";

export function isNotificationSupported(): boolean {
  return typeof window !== "undefined" && "Notification" in window;
}

export function getNotificationPermission(): NotificationPermission | "unsupported" {
  if (!isNotificationSupported()) return "unsupported";
  return Notification.permission;
}

export function isPushEnabled(): boolean {
  if (!isNotificationSupported()) return false;
  if (Notification.permission !== "granted") return false;
  const stored = localStorage.getItem(STORAGE_KEY);
  return stored === null ? true : stored === "true";
}

export function setPushEnabled(enabled: boolean): void {
  localStorage.setItem(STORAGE_KEY, enabled ? "true" : "false");
}

export async function requestNotificationPermission(): Promise<boolean> {
  if (!isNotificationSupported()) return false;
  try {
    const permission = await Notification.requestPermission();
    if (permission === "granted") {
      setPushEnabled(true);
      return true;
    }
    setPushEnabled(false);
    return false;
  } catch (e) {
    console.warn("Échec de la demande de permission de notification:", e);
    return false;
  }
}

export function sendNotification(
  title: string,
  options?: {
    body?: string;
    icon?: string;
    tag?: string;
    silent?: boolean;
    data?: unknown;
  },
): boolean {
  if (!isPushEnabled()) return false;
  try {
    new Notification(title, {
      icon: "/icons/icon-192.png",
      badge: "/icons/icon-192.png",
      ...options,
    });
    return true;
  } catch (e) {
    console.warn("Échec de l'envoi de la notification:", e);
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
