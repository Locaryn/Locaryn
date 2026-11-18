/**
 * La veille de ce qui arrive sans que la personne l'ait demandé, et qui mérite
 * une notification du système : un modèle déchargé faute d'activité (groupe
 * « Modèles en mémoire »), un appareil qui frappe ou qui vient de se connecter
 * (« Sécurité »), le disque des modèles qui se remplit (« Entretien »).
 *
 * Les réglages de Notifications promettaient ces trois groupes depuis
 * longtemps ; rien ne les émettait. Chaque notification passe par
 * `notifierSysteme`, qui applique les préférences du groupe et se tait quand
 * la fenêtre est regardée.
 */

import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { type AccountDevice, type DriveInfo, EVENEMENT_EJECTION, core, coreMode } from "./core";
import { notifierSysteme } from "./osNotify";

/** En dessous, un modèle de taille courante ne tient plus : on prévient. */
export const SEUIL_DISQUE_BAS = 10 * 1024 ** 3;
/** Au-dessus, l'alerte se réarme : quelques octets libérés ne la relancent pas. */
export const SEUIL_DISQUE_REARME = 15 * 1024 ** 3;

const INTERVALLE_DISQUE_MS = 10 * 60_000;
const INTERVALLE_SECURITE_MS = 30_000;

const MOTEURS: Record<string, string> = {
  llama_cpp: "llama.cpp",
  lmstudio: "LM Studio",
  ollama: "Ollama",
};

function gigaoctets(octets: number): string {
  return `${(octets / 1024 ** 3).toFixed(1).replace(".", ",")} Go`;
}

/** Faut-il prévenir, et l'alerte reste-t-elle armée ? Pur, donc testable. */
export function etatDisque(d: DriveInfo, arme: boolean): { prevenir: boolean; arme: boolean } {
  if (d.free_bytes < SEUIL_DISQUE_BAS) return { prevenir: arme, arme: false };
  if (d.free_bytes > SEUIL_DISQUE_REARME) return { prevenir: false, arme: true };
  return { prevenir: false, arme };
}

/** La connexion la plus récente d'un autre appareil que celui-ci, en millisecondes. */
export function derniereConnexionAutre(devices: AccountDevice[]): number {
  let plusRecente = 0;
  for (const d of devices) {
    if (d.current || d.revoked_at) continue;
    const at = d.history[0]?.at;
    const t = at ? Date.parse(at) : Number.NaN;
    if (!Number.isNaN(t) && t > plusRecente) plusRecente = t;
  }
  return plusRecente;
}

/** Une erreur répétée à chaque passage ne s'écrit qu'une fois. */
function journaliserUneFois(dejaVues: Set<string>, contexte: string) {
  return (e: unknown) => {
    const message = `${contexte} : ${e instanceof Error ? e.message : String(e)}`;
    if (dejaVues.has(message)) return;
    dejaVues.add(message);
    console.warn(`[veille] ${message}`);
  };
}

function veillerDisque(erreurs: Set<string>): () => void {
  let arme = true;
  const passer = () =>
    core
      .espaceDisqueModeles()
      .then((d) => {
        if (!d) return;
        const suite = etatDisque(d, arme);
        arme = suite.arme;
        if (suite.prevenir) {
          void notifierSysteme(
            "Espace disque bas",
            `Il reste ${gigaoctets(d.free_bytes)} sur ${d.mount}, le disque des modèles.`,
            "maintenance",
          );
        }
      })
      .catch(journaliserUneFois(erreurs, "espace disque"));
  void passer();
  const t = window.setInterval(() => void passer(), INTERVALLE_DISQUE_MS);
  return () => window.clearInterval(t);
}

/** La demande d'appairage déjà annoncée : une même demande ne sonne qu'une fois. */
let appairageSignale: string | null = null;

function veillerAppairage(erreurs: Set<string>): Promise<void> {
  return core
    .pairingState()
    .then((p) => {
      const cle = p.pending && p.announced ? `${p.device ?? ""}@${p.ip ?? ""}` : null;
      if (cle && cle !== appairageSignale) {
        void notifierSysteme(
          "Un appareil demande à s'appairer",
          `${p.device ?? "Appareil sans nom"}${p.ip ? ` (${p.ip})` : ""}. Le code est dans Réglages > Serveur & fonctions.`,
          "security",
        );
      }
      appairageSignale = cle;
    })
    .catch(journaliserUneFois(erreurs, "appairage"));
}

function veillerConnexions(erreurs: Set<string>, vu: { dernier: number | null }): Promise<void> {
  return core
    .machineAccountDevices()
    .then((r) => {
      const dernier = derniereConnexionAutre(r.devices);
      // Le premier passage pose la référence : rien d'ancien n'est annoncé.
      if (vu.dernier !== null && dernier > vu.dernier) {
        const d = r.devices.find((x) => Date.parse(x.history[0]?.at ?? "") === dernier);
        const ou = d?.history[0]?.location ?? d?.history[0]?.ip ?? null;
        void notifierSysteme(
          "Nouvelle connexion au compte",
          `${d?.label ?? "Un appareil"}${ou ? `, depuis ${ou}` : ""}.`,
          "security",
        );
      }
      vu.dernier = Math.max(dernier, vu.dernier ?? 0);
    })
    .catch(journaliserUneFois(erreurs, "appareils"));
}

function veillerSecurite(erreurs: Set<string>): () => void {
  const vu: { dernier: number | null } = { dernier: null };
  const passer = () => {
    void veillerAppairage(erreurs);
    void veillerConnexions(erreurs, vu);
  };
  passer();
  const t = window.setInterval(passer, INTERVALLE_SECURITE_MS);
  return () => window.clearInterval(t);
}

function veillerEjections(erreurs: Set<string>): () => void {
  let fini = false;
  let arreter: (() => void) | null = null;
  listen<string>(EVENEMENT_EJECTION, (ev) => {
    void notifierSysteme(
      "Modèle déchargé",
      `${MOTEURS[ev.payload] ?? ev.payload} a libéré la mémoire après une longue inactivité. Le prochain message le recharge.`,
      "model_lifecycle",
    );
  })
    .then((stop) => {
      if (fini) stop();
      else arreter = stop;
    })
    .catch(journaliserUneFois(erreurs, "éjections"));
  return () => {
    fini = true;
    arreter?.();
  };
}

/** Monte la veille pour la vie de l'application de bureau. */
export function useVeilleSysteme(): void {
  useEffect(() => {
    if (coreMode !== "tauri") return;
    const erreurs = new Set<string>();
    const arrets = [veillerDisque(erreurs), veillerSecurite(erreurs), veillerEjections(erreurs)];
    return () => {
      for (const arreter of arrets) arreter();
    };
  }, []);
}
