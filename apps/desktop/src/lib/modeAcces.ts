import { useSyncExternalStore } from "react";
import type { PairingMode } from "./core";

/**
 * Le mode d'accès choisi pour le serveur, partagé entre le sélecteur de la
 * section « Service & Mode Serveur », le panneau d'appairage et le QR de
 * connexion d'un compte : un seul choix, trois endroits qui le suivent.
 *
 * `segment` vaut `"local"` ou l'identifiant d'un segment apporté par une
 * extension (`extension:contribution`). L'arrêt du serveur n'est pas un
 * segment : c'est l'état du service qui le dit.
 *
 * `transport` est le dernier code qu'un panneau a demandé au service : le
 * mode (`public`, `tunnel`) et son adresse. C'est lui que le QR de connexion
 * d'un compte réutilise, sans rien savoir de l'extension qui l'a produit.
 */
export interface ModeAcces {
  segment: string;
  transport: { mode: PairingMode; url?: string } | null;
}

const CLE = "locaryn.server.segment";

function lireSegment(): string {
  try {
    return window.localStorage.getItem(CLE) || "local";
  } catch (e) {
    console.warn("mode d'accès illisible, retour au réseau local", e);
    return "local";
  }
}

let etat: ModeAcces = { segment: lireSegment(), transport: null };
const abonnes = new Set<() => void>();

function publier(suivant: ModeAcces) {
  etat = suivant;
  for (const a of abonnes) a();
}

function abonner(a: () => void): () => void {
  abonnes.add(a);
  return () => abonnes.delete(a);
}

export function choisirSegment(segment: string) {
  if (segment === etat.segment) return;
  try {
    window.localStorage.setItem(CLE, segment);
  } catch (e) {
    console.warn("mode d'accès non mémorisé", e);
  }
  // Le transport appartenait à l'ancien segment.
  publier({ segment, transport: null });
}

export function annoncerTransport(mode: PairingMode, url?: string) {
  const t = etat.transport;
  if (t && t.mode === mode && t.url === url) return;
  publier({ ...etat, transport: mode === "local" ? null : { mode, url } });
}

export function useModeAcces(): ModeAcces {
  return useSyncExternalStore(abonner, () => etat);
}
