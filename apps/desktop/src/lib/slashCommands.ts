import type { IconName } from "@locaryn/ui-core";
// Slash commands for the chat composer: type "/" to get a palette of the app's
// actions without hunting through toolbars. Purely declarative — the panel
// supplies the handlers, so this list stays testable and easy to extend.

export type SlashAction =
  | "documents"
  | "json"
  | "reasoning-off"
  | "reasoning-high"
  | "model"
  | "settings"
  | "new-chat"
  | "plan"
  | "workflow"
  | "loop"
  | "clear"
  /** Commande apportee par un plugin : le corps est resolu par le backend. */
  | "extension";

/** Values a command accepts after a space (Tab/Enter completes them). */
export interface SlashArg {
  value: string;
  label: string;
  hint: string;
}

export interface SlashCommand {
  /** Typed after the slash, without it. */
  name: string;
  /** Extra words that also match (search is fuzzy over name + aliases). */
  aliases: string[];
  /** Un nom du jeu d'icônes partagé, jamais un caractère. */
  icon: IconName;
  label: string;
  hint: string;
  action: SlashAction;
  /** Optional arguments proposed once the user types a space. */
  args?: SlashArg[];
  /** Required capability for this command to be available. */
  requiredCapability?: string;
  /**
   * Present uniquement pour `action: "extension"`. Nom qualifie
   * `<plugin>:<commande>` a passer a `resolve_extension_command`.
   */
  extension?: string;
}

/**
 * Ce que des commandes posees devant une demande changent a son execution.
 *
 * `/workflow` et `/loop` ne sont pas des actions qui remplacent le message :
 * elles le **modifient**. On peut donc les cumuler — `/workflow /loop 3 repare
 * le parseur` impose une orchestration et la rejoue jusqu'a trois fois — la ou
 * une commande ordinaire consomme tout le message.
 */
export interface SlashModifiers {
  /** Imposer l'orchestration, sans laisser le modele en decider. */
  force: boolean;
  /** Nombre de tentatives quand la verification echoue. `null` = defaut. */
  loops: number | null;
  /** Ce qui reste de la demande, les modificateurs retires. */
  rest: string;
}

/** Les noms qui declenchent chaque modificateur, alias compris. */
const MODS_FORCE = new Set(["workflow", "orchestre", "orchestration"]);
const MODS_LOOP = new Set(["loop", "boucle", "repete", "répète"]);

/**
 * Detache les modificateurs poses au debut d'une demande.
 *
 * Boucle tant que le texte commence par l'un d'eux, pour qu'ils se cumulent
 * dans n'importe quel ordre. Le nombre qui suit `/loop` est avale seulement
 * s'il en est un : `/loop repare le bug` garde « repare le bug » comme demande
 * et retombe sur le nombre de tentatives par defaut.
 */
export function parseModifiers(input: string): SlashModifiers {
  let rest = input.trim();
  let force = false;
  let loops: number | null = null;

  for (;;) {
    const m = rest.match(/^\/([\p{L}]+)\s*/u);
    if (!m) break;
    const nom = m[1].toLowerCase();
    if (MODS_FORCE.has(nom)) {
      force = true;
      rest = rest.slice(m[0].length);
      continue;
    }
    if (MODS_LOOP.has(nom)) {
      rest = rest.slice(m[0].length);
      const n = rest.match(/^(\d+)\s*/);
      if (n) {
        // Borne haute assumee : au-dela de cinq reprises, ce n'est plus une
        // verification qui echoue, c'est une demande a reformuler.
        loops = Math.min(5, Math.max(2, Number(n[1])));
        rest = rest.slice(n[0].length);
      } else {
        loops = 3;
      }
      continue;
    }
    break;
  }

  return { force, loops, rest: rest.trim() };
}

/** Map a typed argument to a resolution, or null when it isn't one. */
export function argToSize(arg: string): number | null {
  const a = arg.trim().toLowerCase();
  if (/^\d{3,4}$/.test(a)) {
    const n = Number(a);
    return n >= 128 && n <= 2048 ? n : null;
  }
  switch (a) {
    case "brouillon":
    case "draft":
      return 256;
    case "standard":
      return 512;
    case "haute":
    case "high":
      return 768;
    case "max":
    case "maximale":
      return 1024;
    default:
      return null;
  }
}

export const SLASH_COMMANDS: SlashCommand[] = [
  {
    name: "plan",
    aliases: ["etapes", "étapes"],
    icon: "list-bullets",
    label: "Plan par étapes",
    hint: "Le modèle décide s'il faut un plan, et l'exécute le cas échéant",
    action: "plan",
  },
  {
    name: "workflow",
    aliases: ["orchestre", "orchestration"],
    icon: "extensions",
    label: "Forcer un workflow",
    hint: "Orchestration imposée, même si le modèle la juge inutile",
    action: "workflow",
  },
  {
    name: "loop",
    aliases: ["boucle", "repete", "répète"],
    icon: "refresh",
    label: "Répéter jusqu'à ce que ça passe",
    hint: "Rejoue la demande jusqu'à ce qu'elle se vérifie (2 à 5 fois)",
    action: "loop",
    args: [
      { value: "2", label: "2 fois", hint: "deux tentatives au plus" },
      { value: "3", label: "3 fois", hint: "le défaut" },
      { value: "5", label: "5 fois", hint: "pour ce qui résiste" },
    ],
  },
  {
    name: "documents",
    aliases: ["rag", "doc", "connaissance"],
    icon: "models",
    label: "Base de connaissances",
    hint: "Indexer des documents (RAG)",
    action: "documents",
    requiredCapability: "rag-qa",
  },
  {
    name: "json",
    aliases: ["structure", "format"],
    icon: "code",
    label: "Réponse JSON",
    hint: "Force une sortie JSON valide",
    action: "json",
  },
  {
    name: "rapide",
    aliases: ["off", "sans-reflexion"],
    icon: "speed",
    label: "Réflexion désactivée",
    hint: "Réponses directes, plus rapides",
    action: "reasoning-off",
  },
  {
    name: "reflechir",
    aliases: ["réfléchir", "thinking", "raisonnement"],
    icon: "memory",
    label: "Réflexion élevée",
    hint: "Le modèle raisonne davantage",
    action: "reasoning-high",
  },
  {
    name: "modele",
    aliases: ["modèle", "model", "changer"],
    icon: "forward",
    label: "Changer de modèle",
    hint: "Choisir parmi les modèles installés",
    action: "model",
  },
  {
    name: "parametres",
    aliases: ["paramètres", "settings", "options"],
    icon: "settings",
    label: "Paramètres",
    hint: "Ouvrir les réglages de l'application",
    action: "settings",
  },
  {
    name: "nouveau",
    aliases: ["new", "chat"],
    icon: "star",
    label: "Nouvelle conversation",
    hint: "Repartir de zéro",
    action: "new-chat",
  },
  {
    name: "effacer",
    aliases: ["clear", "vider"],
    icon: "trash",
    label: "Effacer l'affichage",
    hint: "Vide la vue (l'historique est conservé)",
    action: "clear",
  },
];

/** What the palette should show: commands, or the arguments of one command. */
export type SlashSuggestion =
  | { kind: "commands"; items: SlashCommand[] }
  | { kind: "args"; command: SlashCommand; items: SlashArg[] };

/** Parse the composer text into palette suggestions (null = not a slash query). */
export function matchSlashInput(
  input: string,
  extra: SlashCommand[] = [],
  activeCapabilities: string[] = [],
): SlashSuggestion | null {
  if (!input.startsWith("/")) return null;
  const rest = input.slice(1);
  const spaceAt = rest.indexOf(" ");
  if (spaceAt >= 0) {
    // "/image hau" → argument completion for the resolved command.
    const name = rest.slice(0, spaceAt);
    const partial = rest
      .slice(spaceAt + 1)
      .trim()
      .toLowerCase();
    const rawPool = extra.length ? [...SLASH_COMMANDS, ...extra] : SLASH_COMMANDS;
    const pool = rawPool.filter(
      (c) => !c.requiredCapability || activeCapabilities.includes(c.requiredCapability),
    );
    const cmd = pool.find((c) => c.name === name || c.aliases.includes(name));
    if (!cmd?.args) return null;
    const items = partial
      ? cmd.args.filter(
          (a) => a.value.startsWith(partial) || a.label.toLowerCase().includes(partial),
        )
      : cmd.args;
    return items.length ? { kind: "args", command: cmd, items } : null;
  }
  const items = matchSlash(input, extra, activeCapabilities) ?? [];
  return items.length ? { kind: "commands", items } : null;
}

/**
 * Text typed in the composer → matching commands, or null when not a slash query.
 *
 * `extra` porte les commandes apportees par les plugins actifs. Elles sont
 * fournies par l'appelant plutot que codees ici : la liste change a chaque
 * installation, et ce module doit rester une fonction pure.
 */
export function matchSlash(
  input: string,
  extra: SlashCommand[] = [],
  activeCapabilities: string[] = [],
): SlashCommand[] | null {
  if (!input.startsWith("/")) return null;
  // Only a single leading token counts as a command query.
  const q = input.slice(1);
  if (/\s/.test(q)) return null;
  const norm = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
  const nq = norm(q);
  const rawAll = extra.length ? [...SLASH_COMMANDS, ...extra] : SLASH_COMMANDS;
  const all = rawAll.filter(
    (c) => !c.requiredCapability || activeCapabilities.includes(c.requiredCapability),
  );
  if (!nq) return all;
  return all.filter(
    (c) =>
      norm(c.name).includes(nq) ||
      c.aliases.some((a) => norm(a).includes(nq)) ||
      norm(c.label).includes(nq),
  );
}

/**
 * Le texte est-il une commande de plugin ecrite en entier, arguments compris ?
 *
 * Les commandes integrees se declenchent depuis la palette, qui reste ouverte
 * parce qu'elles declarent leurs arguments. Une commande de plugin n'en declare
 * aucun : la palette se ferme des le premier espace, et « /plugin:cmd chemin »
 * partirait tel quel comme message. Ce test rattrape ce cas.
 */
export function matchExtensionCommand(input: string, extra: SlashCommand[]): SlashCommand | null {
  if (!input.startsWith("/") || extra.length === 0) return null;
  const rest = input.slice(1);
  const space = rest.indexOf(" ");
  const name = (space >= 0 ? rest.slice(0, space) : rest).trim();
  if (!name) return null;
  return extra.find((c) => c.name === name || c.aliases.includes(name)) ?? null;
}
