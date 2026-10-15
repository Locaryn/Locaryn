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
  | "goal"
  | "tester"
  | "revue"
  | "init"
  | "compresser"
  | "aide"
  | "clear"
  /** Commande apportee par un plugin : le corps est resolu par le backend. */
  | "extension"
  /** Un skill : choisi, il s'insere en « @nom », que le backend impose au modele. */
  | "skill";

/** Minuscules sans accents : « Étapes » et « etapes » sont le meme mot. */
function normWord(s: string): string {
  return s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** Values a command accepts after a space (Tab/Enter completes them). */
export interface SlashArg {
  value: string;
  label: string;
  hint: string;
}

export interface SlashCommand {
  /** Typed after the slash, without it. */
  name: string;
  /**
   * Nom affiche quand l'interface est en anglais. Facultatif : une commande
   * dont le nom francais est aussi le nom anglais (« plan ») n'en a pas besoin.
   * Les deux graphies restent acceptees a la frappe, quelle que soit la langue.
   */
  nameEn?: string;
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
  /** `/objectif` : enchainer jusqu'a ce que les conditions soient remplies. */
  goal: boolean;
  /** `/plan` : decomposer et executer etape par etape. */
  plan: boolean;
  /** Ce qui reste de la demande, les modificateurs retires. */
  rest: string;
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
    name: "objectif",
    nameEn: "goal",
    aliases: ["goal", "but", "jusquau", "jusqu'au bout"],
    icon: "crosshair",
    label: "Objectif à atteindre",
    hint: "Le modèle enchaîne sans rendre la main jusqu'à ce que les conditions soient remplies",
    action: "goal",
  },
  {
    name: "tester",
    aliases: ["humain", "simuler", "qa", "essai"],
    icon: "cursor-click",
    label: "Tester comme un humain",
    hint: "Le modèle joue un utilisateur qui essaie l'application, l'installation ou la page",
    action: "tester",
  },
  {
    name: "revue",
    aliases: ["review", "relecture", "audit"],
    icon: "magnifying-glass",
    label: "Revue de code",
    hint: "Relire les changements du projet : bugs, sécurité, simplifications",
    action: "revue",
  },
  {
    name: "init",
    aliases: ["contexte", "projet", "agents"],
    icon: "file-text",
    label: "Décrire le projet",
    hint: "Analyser le dossier et écrire LOCARYN.md, le contexte que le modèle relira",
    action: "init",
  },
  {
    name: "compresser",
    aliases: ["compact", "resume", "résumé"],
    icon: "stack",
    label: "Compresser la conversation",
    hint: "Les vieux échanges deviennent un résumé, les derniers restent",
    action: "compresser",
  },
  {
    name: "aide",
    aliases: ["help", "commandes", "?"],
    icon: "question",
    label: "Aide",
    hint: "Ce que fait chaque commande",
    action: "aide",
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

/**
 * Le nom a montrer dans la langue de l'interface.
 *
 * Les deux graphies restent acceptees a la frappe, quelle que soit la langue :
 * seul l'affichage suit le reglage. Le nom francais est celui de reference.
 */
export function commandName(c: SlashCommand, lang: "fr" | "en"): string {
  return lang === "en" ? (c.nameEn ?? c.name) : c.name;
}

/**
 * Les mots qui declenchent chaque modificateur, deduits des commandes.
 *
 * Ils etaient ecrits a la main, ici, en plus des commandes de la palette : les
 * deux listes pouvaient diverger, et un nom que la palette propose se retrouvait
 * refuse a la frappe — la ligne entiere partait alors au modele comme un message
 * ordinaire. Une seule source, donc : `SLASH_COMMANDS`.
 */
const MOTS_MODIFICATEURS = new Map<SlashAction, Set<string>>();

/** Les mots reconnus pour une action : nom courant, nom anglais et alias. */
function motsDeLaction(action: SlashAction): Set<string> {
  const connu = MOTS_MODIFICATEURS.get(action);
  if (connu) return connu;
  const mots = new Set<string>();
  for (const c of SLASH_COMMANDS) {
    if (c.action !== action) continue;
    for (const mot of [c.name, c.nameEn, ...c.aliases]) {
      // Un mot qui contient un espace (« jusqu'au bout ») ne s'ecrit pas d'un
      // trait apres un seul « / » : ce n'est pas un modificateur tapable.
      if (mot && !/\s/.test(mot)) mots.add(normWord(mot));
    }
  }
  MOTS_MODIFICATEURS.set(action, mots);
  return mots;
}

/**
 * Detache les modificateurs, ou qu'ils soient dans la demande.
 *
 * « continue /goal » vaut « /goal continue » : on ne doit pas etre oblige de
 * poser la commande au debut du message. Chaque mot reconnu est retire du
 * texte, et ce qui reste devient la demande. Le nombre qui suit `/loop` est
 * avale seulement s'il en est un : `/loop repare le bug` garde « repare le
 * bug » comme demande et retombe sur le nombre de tentatives par defaut.
 *
 * Le mot doit commencer un mot (debut du texte, ou un espace devant) : une
 * adresse qui finit par « /goal » n'est pas une commande.
 */
export function parseModifiers(input: string): SlashModifiers {
  let force = false;
  let loops: number | null = null;
  let goal = false;
  let plan = false;

  const motif = /(?:^|\s)\/([\p{L}]+)/gu;
  let rest = "";
  let curseur = 0;
  for (const m of input.matchAll(motif)) {
    const nom = normWord(m[1]);
    const but = motsDeLaction("goal").has(nom);
    const orchestre = motsDeLaction("workflow").has(nom);
    const boucle = motsDeLaction("loop").has(nom);
    const planifie = motsDeLaction("plan").has(nom);
    if (!but && !orchestre && !boucle && !planifie) continue;
    rest += input.slice(curseur, m.index);
    curseur = m.index + m[0].length;
    // La ponctuation collee au mot part avec lui : « continue /goal. » ne doit
    // pas laisser un point orphelin.
    if (".,;:!?".includes(input.charAt(curseur))) curseur += 1;
    if (but) {
      goal = true;
      continue;
    }
    if (orchestre) {
      force = true;
      continue;
    }
    if (planifie) {
      plan = true;
      continue;
    }
    // Le nombre qui suit `/loop` est avale seulement s'il en est un.
    const n = input.slice(curseur).match(/^\s*(\d+)(?=\s|$)/);
    if (n) {
      // Borne haute assumee : au-dela de cinq reprises, ce n'est plus une
      // verification qui echoue, c'est une demande a reformuler.
      loops = Math.min(5, Math.max(2, Number(n[1])));
      curseur += n[0].length;
    } else {
      loops = 3;
    }
  }
  rest += input.slice(curseur);

  return {
    force,
    loops,
    goal,
    plan,
    // Les trous laisses par les mots retires se referment : « continue  et
    // verifie » redevient lisible. Les sauts de ligne et l'indentation d'un
    // extrait colle dans la demande sont conserves.
    rest: rest
      .replace(/[ \t]{2,}/g, " ")
      .replace(/[ \t]+$/gm, "")
      .replace(/\n{3,}/g, "\n\n")
      .trim(),
  };
}

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
 * Le « /mot » en cours de frappe à la fin du texte, qu'il soit au début
 * du message ou au milieu/à la fin (après un espace ou une autre commande) :
 * « /g » → « g »
 * « /goal /p » → « p »
 * « /goal / » → « »
 * « explique ce code /plan » → « plan »
 * `null` quand le texte ne finit pas par un slash en cours de frappe.
 */
export function slashEnFin(input: string): string | null {
  const m = /(?:^|\s)\/([\w:.-]*)$/.exec(input);
  return m ? m[1] : null;
}

/**
 * Le « /mot » en cours de frappe à l'endroit du curseur, même quand du texte
 * le suit : « /pl| corrige le bug » → « pl ». Le mot doit être séparé de ce
 * qui suit par un espace (ou finir le texte), et précédé d'un début de ligne
 * ou d'un espace — une adresse « a/b » n'est pas une commande.
 * Renvoie le mot et ses bornes dans le texte, `null` sinon.
 */
export function slashAuCurseur(
  input: string,
  cursor: number,
): { mot: string; debut: number; fin: number } | null {
  const avant = input.slice(0, cursor);
  const m = /(?:^|\s)\/([\w:.-]*)$/.exec(avant);
  if (!m) return null;
  const apres = input.slice(cursor);
  const suite = /^[\w:.-]*/.exec(apres)?.[0] ?? "";
  const reste = apres.slice(suite.length);
  if (reste.length > 0 && !/^\s/.test(reste)) return null;
  const debut = cursor - m[1].length - 1;
  return { mot: m[1] + suite, debut, fin: cursor + suite.length };
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
  const nq = normWord(q);
  const rawAll = extra.length ? [...SLASH_COMMANDS, ...extra] : SLASH_COMMANDS;
  const all = rawAll.filter(
    (c) => !c.requiredCapability || activeCapabilities.includes(c.requiredCapability),
  );
  if (!nq) return all;
  return all.filter(
    (c) =>
      normWord(c.name).includes(nq) ||
      c.aliases.some((a) => normWord(a).includes(nq)) ||
      normWord(c.label).includes(nq),
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
  // Une commande integree garde son nom : sans ce garde-fou, un plugin qui
  // declare « objectif » prend la place de `/objectif` — la commande integree
  // parait alors cassee, alors que son alias `/goal` repond encore. Le plugin
  // reste joignable par son nom qualifie (`/plugin:objectif`).
  const reserve = normWord(name);
  const integr = SLASH_COMMANDS.some(
    (c) =>
      normWord(c.name) === reserve ||
      normWord(c.nameEn ?? "") === reserve ||
      c.aliases.some((a) => normWord(a) === reserve),
  );
  if (integr) return null;
  return extra.find((c) => c.name === name || c.aliases.includes(name)) ?? null;
}

/**
 * Ce que certaines commandes envoient au modèle. `{cible}` reçoit ce qui suit
 * la commande ; vide, le gabarit dit quoi faire sans.
 */
export const GABARITS: Partial<Record<SlashAction, (cible: string) => string>> = {
  tester: (cible) =>
    [
      `Agis comme un utilisateur humain qui découvre ${cible || "l'application ou l'installation en cours"} et la teste pour de vrai. `,
      "Utilise les outils dont tu disposes (navigateur, contrôle du bureau, terminal) pour la parcourir comme le ferait une personne : ",
      "suivre l'installation ou le premier lancement, cliquer, remplir les formulaires, revenir en arrière, essayer les cas limites. ",
      "Ne corrige rien pendant le test. À la fin, rends un rapport : ce qui marche, chaque friction ou bug avec les étapes pour le reproduire, ",
      "et ce qu'un utilisateur ne comprendrait pas.",
    ].join(""),
  revue: (cible) =>
    [
      `Fais une revue de code ${cible ? `de ${cible}` : "des changements en cours du projet (git diff, puis les fichiers touchés)"}. `,
      "Cherche d'abord les bugs réels (avec le scénario qui les déclenche), puis les failles de sécurité, puis ce qui peut être simplifié. ",
      "Lis le code avant de conclure ; classe chaque point par gravité et cite le fichier et la ligne. Ne modifie rien.",
    ].join(""),
  init: (cible) =>
    [
      `Analyse ${cible || "le dossier de travail"} : structure, langages, commandes de build et de test, conventions, points d'attention. `,
      "Écris ensuite un fichier LOCARYN.md à la racine, court et factuel, qui donne à un assistant ce qu'il doit savoir pour y travailler. ",
      "S'il existe déjà, mets-le à jour au lieu de l'écraser.",
    ].join(""),
};
