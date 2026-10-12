// Le presse-papiers, par l'application plutôt que par la page.
//
// `navigator.clipboard.readText` déclenche sous Windows la demande
// d'autorisation de WebView2, un bandeau de navigateur en haut de la fenêtre
// à chaque « Coller ». Dans l'application, on passe par le système ; hors de
// l'application (aperçu web), par la page, faute de mieux.

import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./core";

/** Le texte du presse-papiers. */
export async function lirePressePapiers(): Promise<string> {
  if (isTauri) return invoke<string>("lire_presse_papiers");
  return navigator.clipboard.readText();
}

/** Copier `texte`. Rejette si ni le système ni la page ne l'acceptent. */
export async function ecrirePressePapiers(texte: string): Promise<void> {
  if (isTauri) {
    try {
      await invoke<void>("ecrire_presse_papiers", { texte });
      return;
    } catch (e) {
      console.warn("presse-papiers du système indisponible, repli sur la page :", e);
    }
  }
  await navigator.clipboard.writeText(texte);
}
