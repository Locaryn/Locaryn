import { isTauri } from "./core";

/**
 * F5 et Ctrl+R ne rechargent pas l'application.
 *
 * Dans le webview, ces touches rechargent l'interface comme une page web : la
 * réponse en cours disparaît, ses appels d'outils avec. C'est arrivé avec un
 * modèle qui pilotait l'ordinateur — il visait Roblox Studio, la touche est
 * tombée sur Locaryn. Une application de bureau ne se recharge pas sous les
 * doigts. Le navigateur de développement garde son F5.
 */
function estUnRechargement(e: KeyboardEvent): boolean {
  if (e.key === "F5") return true;
  return (e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "r";
}

if (isTauri) {
  window.addEventListener(
    "keydown",
    (e) => {
      if (estUnRechargement(e)) e.preventDefault();
    },
    { capture: true },
  );
}
