// Les rapports anonymes envoyés à l'équipe : erreur, note d'une réponse,
// problème décrit par la personne. Le nettoyage (chemins, e-mails, clés,
// prompts) se fait côté Rust, dans `rapports.rs` et `journaux.rs` : rien ne
// part sans y passer.

import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./core";

export type NoteReponse = "up" | "down";

/** Un pouce baissé : bug critique, ou réponse simplement mauvaise. */
export type GraviteNote = "critique" | "info";

/** L'échange qu'on note : la demande, la réponse, les outils appelés. */
export interface EchangeNote {
  question: string;
  reponse: string;
  outils: string[];
}

export interface EtatRapports {
  disponible: boolean;
  actif: boolean;
}

interface Envoi {
  message: string | null;
  genre: "erreur" | "note" | "probleme";
  note: NoteReponse | null;
  gravite: GraviteNote | null;
  echange: EchangeNote | null;
  modele: string | null;
}

const HORS_APP: EtatRapports = { disponible: false, actif: false };

async function envoyer(envoi: Envoi): Promise<void> {
  if (!isTauri) throw new Error("Envoi possible seulement dans l'application.");
  await invoke<void>("rapports_envoyer", { ...envoi });
}

/** L'état de l'envoi automatique ; le modèle utilisé accompagnera les rapports. */
export async function etatRapports(modele?: string | null): Promise<EtatRapports> {
  if (!isTauri) return HORS_APP;
  return invoke<EtatRapports>("rapports_etat", { modele: modele ?? null });
}

/** Le bouton « Envoyer les journaux » d'une erreur. */
export function envoyerErreur(message: string, modele?: string | null): Promise<void> {
  return envoyer({
    message,
    genre: "erreur",
    note: null,
    gravite: null,
    echange: null,
    modele: modele ?? null,
  });
}

/** Un pouce sur une réponse : l'échange noté part avec les journaux. */
export function envoyerNote(
  note: NoteReponse,
  gravite: GraviteNote,
  echange: EchangeNote,
  modele?: string | null,
): Promise<void> {
  return envoyer({ message: null, genre: "note", note, gravite, echange, modele: modele ?? null });
}

/** « J'ai un problème » : la description, et l'échange en cours si la personne le joint. */
export function envoyerProbleme(
  description: string,
  gravite: GraviteNote,
  echange: EchangeNote | null,
): Promise<void> {
  return envoyer({
    message: description,
    genre: "probleme",
    note: null,
    gravite,
    echange,
    modele: null,
  });
}

/** Ce qu'il faut d'un élément du fil pour retrouver un échange. */
export interface ItemFil {
  id: string;
  kind: string;
  role?: string;
  text?: string;
  tool?: string;
}

function estQuestion(it: ItemFil): boolean {
  return it.kind === "msg" && it.role === "user";
}

/**
 * L'échange autour d'une réponse : la demande qui la précède, la réponse
 * (sans la réflexion), et les outils appelés jusqu'à la demande suivante.
 */
export function echangeDe(
  items: readonly ItemFil[],
  idReponse: string,
  reponseSeule: (texte: string) => string,
): EchangeNote | null {
  const i = items.findIndex((it) => it.id === idReponse);
  if (i < 0) return null;
  let debut = i - 1;
  while (debut >= 0 && !estQuestion(items[debut] as ItemFil)) debut--;
  let fin = i + 1;
  while (fin < items.length && !estQuestion(items[fin] as ItemFil)) fin++;
  const outils: string[] = [];
  for (const it of items.slice(debut + 1, fin)) {
    if (it.kind === "tool" && it.tool && !outils.includes(it.tool)) outils.push(it.tool);
  }
  return {
    question: debut >= 0 ? (items[debut]?.text ?? "") : "",
    reponse: reponseSeule(items[i]?.text ?? ""),
    outils,
  };
}

/**
 * Le dernier échange de la conversation ouverte, publié par le fil : le bouton
 * « J'ai un problème » vit dans la barre du bas, loin de la conversation.
 */
let dernierEchange: EchangeNote | null = null;

export function publierDernierEchange(e: EchangeNote | null): void {
  dernierEchange = e;
}

export function lireDernierEchange(): EchangeNote | null {
  return dernierEchange;
}
