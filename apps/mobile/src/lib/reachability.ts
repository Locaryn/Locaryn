/**
 * Une erreur réseau signalée n'importe où dans l'application.
 *
 * Chaque écran (le chat, les extensions, les modèles) attrape ses propres
 * erreurs et les affiche dans son propre toast — c'est correct, chacun sait ce
 * qu'il tentait de faire. Mais quand la cause est la même — le serveur ne
 * répond plus — la réponse est aussi la même partout : proposer de se
 * reconnecter. Plutôt que de câbler cette proposition dans chaque écran, ils
 * signalent l'échec ici, et un seul endroit décide de la montrer.
 */
type Ecouteur = (message: string) => void;
const ecouteurs = new Set<Ecouteur>();

/** Ce que `unreachable()` écrit côté Rust — les deux formes, mode voyage ou non. */
const SIGNES = ["Le serveur ne répond pas", "Vous n'êtes pas connecté"];

/**
 * Signaler une erreur. Si elle ressemble à un serveur injoignable, les
 * écouteurs sont prévenus ; sinon rien ne se passe — un mot de passe refusé
 * n'est pas une raison de proposer une reconnexion réseau.
 */
export function signalerErreur(message: string): void {
  if (SIGNES.some((s) => message.includes(s))) {
    for (const f of ecouteurs) f(message);
  }
}

/** S'abonner aux échecs réseau signalés depuis n'importe quel écran. */
export function surEchecReseau(f: Ecouteur): () => void {
  ecouteurs.add(f);
  return () => ecouteurs.delete(f);
}
