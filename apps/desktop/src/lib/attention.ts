import type { AttentionItem } from "./core";

/**
 * Choisir laquelle des questions en attente s'affiche ici, et maintenant.
 *
 * Une seule à la fois : empiler trois questions au-dessus du champ de saisie
 * remplacerait le message par un formulaire, et la personne ne saurait plus
 * laquelle elle est en train de régler.
 *
 * L'ordre de préférence suit la précision de l'adresse — de la plus précise à
 * la plus vague. Une question posée dans cette conversation passe devant une
 * question posée ailleurs dans le projet, qui passe devant une alerte générale
 * de l'application. Sans cette règle, une panne globale masquerait la question
 * dont la réponse débloque justement le travail en cours.
 *
 * À portée égale, la plus ancienne d'abord : c'est celle qui attend depuis le
 * plus longtemps, et l'ordre stable évite que la bande change de contenu à
 * chaque nouvelle arrivée.
 */
export function attentionPourVue(
  items: readonly AttentionItem[],
  sessionId: string | null,
  projectId: string | null,
): AttentionItem | null {
  const rang = (a: AttentionItem): number => {
    if (sessionId && a.session_id === sessionId) return 0;
    // Une question adressée à une autre conversation ne s'affiche pas ici :
    // y répondre depuis un autre fil ferait arriver la réponse dans un
    // contexte que la personne n'a pas sous les yeux.
    if (a.session_id) return -1;
    if (projectId && a.project_id === projectId) return 1;
    if (a.project_id) return -1;
    return 2;
  };
  let meilleur: AttentionItem | null = null;
  let meilleurRang = Number.POSITIVE_INFINITY;
  for (const a of items) {
    const r = rang(a);
    if (r < 0 || r >= meilleurRang) continue;
    meilleur = a;
    meilleurRang = r;
  }
  return meilleur;
}

/** L'état d'une conversation d'après ce qui l'attend. L'erreur passe devant. */
export function etatDeSession(
  items: readonly AttentionItem[],
  sessionId: string,
): "attente" | "erreur" | null {
  let attente = false;
  for (const a of items) {
    if (a.session_id !== sessionId) continue;
    if (a.urgency === "erreur") return "erreur";
    attente = true;
  }
  return attente ? "attente" : null;
}
