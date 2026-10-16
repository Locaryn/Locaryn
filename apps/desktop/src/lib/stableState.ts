/**
 * Garde la valeur précédente quand la nouvelle a le même contenu.
 *
 * Une relecture périodique (extensions, capacités) renvoie un tableau neuf à
 * chaque fois, même inchangé. Le poser tel quel change son identité et relance
 * tout ce qui en dépend : le Marketplace vidait alors le catalogue des morphs,
 * perdait le filtre choisi et la fiche ouverte. À passer à un `setState`
 * fonctionnel : `setX((prev) => keepIfEqual(prev, next))`.
 */
export function keepIfEqual<T>(prev: T, next: T): T {
  return JSON.stringify(prev) === JSON.stringify(next) ? prev : next;
}
