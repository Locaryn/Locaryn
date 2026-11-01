import type { Message } from "./core";

/** Longueur maximale de ce qu'on montre au modèle pour nommer une conversation. */
const LIMITE = 1600;
const PAR_MESSAGE = 400;

function couper(texte: string, max: number): string {
  const t = texte.replace(/\s+/g, " ").trim();
  return t.length > max ? `${t.slice(0, max)}…` : t;
}

/**
 * Ce qu'on donne au modèle pour renommer une conversation déjà avancée : la
 * première demande (le sujet de départ) et les derniers échanges (là où elle
 * en est), sans les appels d'outils.
 */
export function extraitPourTitre(messages: Message[]): string {
  const echanges = messages.filter(
    (m) => (m.role === "user" || m.role === "assistant") && m.content.trim() !== "",
  );
  if (echanges.length === 0) return "";
  const premier = echanges[0];
  const derniers = echanges.slice(1).slice(-4);
  const lignes = [premier, ...derniers].map(
    (m) => `${m.role === "user" ? "Personne" : "Assistant"} : ${couper(m.content, PAR_MESSAGE)}`,
  );
  return couper(lignes.join("\n"), LIMITE);
}
