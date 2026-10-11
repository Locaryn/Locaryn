/**
 * La barre latérale a été recliquée sur l'écran déjà ouvert. Chaque écran qui
 * a des sous-pages (un dossier de modèles, une section) revient alors à sa
 * page d'accueil : c'est le « retour » qu'on attend de ce geste.
 *
 * `detail` porte l'identifiant de l'écran (`installed`, `settings`…).
 */
export const VUE_RECHOISIE = "locaryn:vue-rechoisie";
