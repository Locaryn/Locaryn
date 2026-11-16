/**
 * Une promesse bornée dans le temps : rejetée si `promesse` n'a pas répondu
 * après `ms` millisecondes. La promesse d'origine continue sa course ; seul
 * l'appelant cesse de l'attendre.
 */
export function avantDelai<T>(promesse: Promise<T>, ms: number, quoi: string): Promise<T> {
  let minuterie: ReturnType<typeof setTimeout> | undefined;
  const delai = new Promise<never>((_, rejeter) => {
    minuterie = setTimeout(() => rejeter(new Error(`${quoi} : pas de réponse en ${ms} ms`)), ms);
  });
  return Promise.race([promesse, delai]).finally(() => {
    if (minuterie !== undefined) clearTimeout(minuterie);
  });
}
