// Le suivi des sous-agents côté interface.
//
// Le service émet `sous-agent` à chaque étape (début, jetons, outils, demande
// d'accord, fin). Ce module tient le centre de tâches à jour — c'est ce que
// montre l'onglet « Tâches » — et relaie deux moments à la conversation qui a
// délégué : une demande d'accord à afficher, et le compte rendu final à lui
// rendre.

import { listen } from "@tauri-apps/api/event";
import { isTauri } from "./core";
import { taskCenter } from "./taskCenter";

/** Ce que le service émet (voir `sous_agents.rs`). */
export interface EvenementSousAgent {
  id: string;
  parent: string;
  nom: string;
  modele: string;
  phase: "debut" | "jeton" | "outil" | "accord" | "fin" | "echec";
  texte: string;
  accord?: Record<string, unknown>;
}

/** Le compte rendu d'un sous-agent, pour la conversation qui l'a lancé. */
export const SOUS_AGENT_FINI = "locaryn:sous-agent-fini";
/** Une demande d'accord d'un sous-agent, à montrer dans sa conversation. */
export const SOUS_AGENT_ACCORD = "locaryn:sous-agent-accord";

export interface SousAgentFini {
  parent: string;
  id: string;
  nom: string;
  ok: boolean;
  texte: string;
}

/** Ce qu'un fil garde au plus, en caractères : la fin suffit à suivre. */
const FIL_MAX = 20_000;

const taches = new Map<string, string>();
let installe = false;

/** Brancher l'écoute une fois, au démarrage de l'application. */
export function suivreSousAgents(): void {
  if (installe || !isTauri) return;
  installe = true;
  void listen<EvenementSousAgent>("sous-agent", (e) => recevoir(e.payload));
}

function recevoir(ev: EvenementSousAgent): void {
  let tache = taches.get(ev.id);
  if (!tache) {
    tache = taskCenter.add({ type: "subagent", label: ev.nom, detail: ev.modele });
    taches.set(ev.id, tache);
    taskCenter.update(tache, { parent: ev.parent, model: ev.modele, transcript: "", tools: [] });
  }
  const t = taskCenter.snapshot().find((x) => x.id === tache);
  switch (ev.phase) {
    case "debut":
      taskCenter.update(tache, { transcript: `Tâche : ${ev.texte}\n\n` });
      break;
    case "jeton":
      taskCenter.update(tache, {
        transcript: `${t?.transcript ?? ""}${ev.texte}`.slice(-FIL_MAX),
      });
      break;
    case "outil":
      taskCenter.update(tache, {
        tools: [...(t?.tools ?? []), ev.texte],
        detail: `${ev.modele} · ${ev.texte}`,
      });
      break;
    case "accord":
      window.dispatchEvent(
        new CustomEvent(SOUS_AGENT_ACCORD, { detail: { parent: ev.parent, accord: ev.accord } }),
      );
      break;
    case "fin":
    case "echec": {
      const ok = ev.phase === "fin";
      if (ok) taskCenter.done(tache, { detail: ev.modele });
      else taskCenter.fail(tache, ev.texte);
      taches.delete(ev.id);
      const detail: SousAgentFini = {
        parent: ev.parent,
        id: ev.id,
        nom: ev.nom,
        ok,
        texte: ev.texte,
      };
      window.dispatchEvent(new CustomEvent(SOUS_AGENT_FINI, { detail }));
      break;
    }
  }
}
