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
/** Le dernier transport obtenu, avec le segment qui l'a produit. */
const CLE_TRANSPORT = "locaryn.server.transport";

function lireSegment(): string {
  try {
    return window.localStorage.getItem(CLE) || "local";
  } catch (e) {
    console.warn("mode d'accès illisible, retour au réseau local", e);
    return "local";
  }
}

const MODES: readonly PairingMode[] = ["local", "public", "tunnel"];

function estMode(v: unknown): v is PairingMode {
  return typeof v === "string" && (MODES as readonly string[]).includes(v);
}

/**
 * Le transport retenu pour ce segment au dernier lancement.
 *
 * Sans lui, le QR de connexion d'un compte ne connaissait le mode Internet
 * qu'une fois le panneau d'appairage ouvert pendant cette session : après un
 * redémarrage, il portait l'adresse du réseau local, en silence, et le
 * téléphone dehors trouvait « Serveur injoignable ».
 */
function lireTransport(segment: string): ModeAcces["transport"] {
  try {
    const brut = window.localStorage.getItem(CLE_TRANSPORT);
    if (!brut) return null;
    const v: unknown = JSON.parse(brut);
    if (typeof v !== "object" || v === null) return null;
    const o = v as { segment?: unknown; mode?: unknown; url?: unknown };
    if (o.segment !== segment || !estMode(o.mode) || o.mode === "local") return null;
    return { mode: o.mode, url: typeof o.url === "string" ? o.url : undefined };
  } catch (e) {
    console.warn("transport mémorisé illisible", e);
    return null;
  }
}

function ecrireTransport(segment: string, transport: ModeAcces["transport"]) {
  try {
    if (transport) {
      window.localStorage.setItem(CLE_TRANSPORT, JSON.stringify({ segment, ...transport }));
    } else {
      window.localStorage.removeItem(CLE_TRANSPORT);
    }
  } catch (e) {
    console.warn("transport non mémorisé", e);
  }
}

const segmentInitial = lireSegment();
let etat: ModeAcces = { segment: segmentInitial, transport: lireTransport(segmentInitial) };
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
  // Le transport appartenait à l'ancien segment ; celui du nouveau, s'il a
  // déjà servi, revient.
  publier({ segment, transport: lireTransport(segment) });
}

export function annoncerTransport(mode: PairingMode, url?: string) {
  const t = etat.transport;
  if (t && t.mode === mode && t.url === url) return;
  // Le panneau du réseau local annonce « local » à son ouverture : il ne doit
  // pas effacer le transport d'un segment distant retenu au dernier lancement.
  if (mode === "local" && etat.segment !== "local") return;
  const transport = mode === "local" ? null : { mode, url };
  ecrireTransport(etat.segment, transport);
  publier({ ...etat, transport });
}

export function useModeAcces(): ModeAcces {
  return useSyncExternalStore(abonner, () => etat);
}
