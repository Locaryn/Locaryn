// Slash commands for the chat composer: type "/" to get a palette of the app's
// actions without hunting through toolbars. Purely declarative — the panel
// supplies the handlers, so this list stays testable and easy to extend.

export type SlashAction =
  | "image"
  | "edit-image"
  | "documents"
  | "json"
  | "reasoning-off"
  | "reasoning-high"
  | "model"
  | "settings"
  | "new-chat"
  | "plan"
  | "clear";

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
  icon: string;
  label: string;
  hint: string;
  action: SlashAction;
  /** Optional arguments proposed once the user types a space. */
  args?: SlashArg[];
}

/** Quality arguments shared by the image commands. */
const QUALITY_ARGS: SlashArg[] = [
  { value: "brouillon", label: "Brouillon · 256px", hint: "Le plus rapide — icônes, essais" },
  { value: "standard", label: "Standard · 512px", hint: "Compromis vitesse/qualité" },
  { value: "haute", label: "Haute · 768px", hint: "Plus détaillé, plus lent" },
  { value: "max", label: "Maximale · 1024px", hint: "Qualité maximale, le plus lent" },
];

/** Map a typed argument to a resolution, or null when it isn't one. */
export function argToSize(arg: string): number | null {
  const a = arg.trim().toLowerCase();
  if (/^\d{3,4}$/.test(a)) {
    const n = Number(a);
    return n >= 128 && n <= 2048 ? n : null;
  }
  switch (a) {
    case "brouillon": case "draft": return 256;
    case "standard": return 512;
    case "haute": case "high": return 768;
    case "max": case "maximale": return 1024;
    default: return null;
  }
}

export const SLASH_COMMANDS: SlashCommand[] = [
  { name: "image", aliases: ["img", "generer", "générer", "dessine"], icon: "🎨", label: "Générer une image", hint: "Ouvre le studio d'image", action: "image", args: QUALITY_ARGS },
  { name: "editer-image", aliases: ["edit", "retouche", "img2img"], icon: "🖼️", label: "Éditer une image", hint: "Transformer une image existante", action: "edit-image", args: QUALITY_ARGS },
  { name: "plan", aliases: ["workflow", "etapes", "étapes"], icon: "\ud83e\udde9", label: "Plan par etapes", hint: "Decompose la demande et l execute etape par etape", action: "plan" },
  { name: "documents", aliases: ["rag", "doc", "connaissance"], icon: "📚", label: "Base de connaissances", hint: "Indexer des documents (RAG)", action: "documents" },
  { name: "json", aliases: ["structure", "format"], icon: "{ }", label: "Réponse JSON", hint: "Force une sortie JSON valide", action: "json" },
  { name: "rapide", aliases: ["off", "sans-reflexion"], icon: "⚡", label: "Réflexion désactivée", hint: "Réponses directes, plus rapides", action: "reasoning-off" },
  { name: "reflechir", aliases: ["réfléchir", "thinking", "raisonnement"], icon: "🧠", label: "Réflexion élevée", hint: "Le modèle raisonne davantage", action: "reasoning-high" },
  { name: "modele", aliases: ["modèle", "model", "changer"], icon: "🔀", label: "Changer de modèle", hint: "Choisir parmi les modèles installés", action: "model" },
  { name: "parametres", aliases: ["paramètres", "settings", "options"], icon: "⚙", label: "Paramètres", hint: "Ouvrir les réglages de l'application", action: "settings" },
  { name: "nouveau", aliases: ["new", "chat"], icon: "✨", label: "Nouvelle conversation", hint: "Repartir de zéro", action: "new-chat" },
  { name: "effacer", aliases: ["clear", "vider"], icon: "🧹", label: "Effacer l'affichage", hint: "Vide la vue (l'historique est conservé)", action: "clear" },
];

/** What the palette should show: commands, or the arguments of one command. */
export type SlashSuggestion =
  | { kind: "commands"; items: SlashCommand[] }
  | { kind: "args"; command: SlashCommand; items: SlashArg[] };

/** Parse the composer text into palette suggestions (null = not a slash query). */
export function matchSlashInput(input: string): SlashSuggestion | null {
  if (!input.startsWith("/")) return null;
  const rest = input.slice(1);
  const spaceAt = rest.indexOf(" ");
  if (spaceAt >= 0) {
    // "/image hau" → argument completion for the resolved command.
    const name = rest.slice(0, spaceAt);
    const partial = rest.slice(spaceAt + 1).trim().toLowerCase();
    const cmd = SLASH_COMMANDS.find(
      (c) => c.name === name || c.aliases.includes(name),
    );
    if (!cmd?.args) return null;
    const items = partial
      ? cmd.args.filter((a) => a.value.startsWith(partial) || a.label.toLowerCase().includes(partial))
      : cmd.args;
    return items.length ? { kind: "args", command: cmd, items } : null;
  }
  const items = matchSlash(input) ?? [];
  return items.length ? { kind: "commands", items } : null;
}

/** Text typed in the composer → matching commands, or null when not a slash query. */
export function matchSlash(input: string): SlashCommand[] | null {
  if (!input.startsWith("/")) return null;
  // Only a single leading token counts as a command query.
  const q = input.slice(1);
  if (/\s/.test(q)) return null;
  const norm = (s: string) =>
    s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
  const nq = norm(q);
  if (!nq) return SLASH_COMMANDS;
  return SLASH_COMMANDS.filter(
    (c) =>
      norm(c.name).includes(nq) ||
      c.aliases.some((a) => norm(a).includes(nq)) ||
      norm(c.label).includes(nq),
  );
}
