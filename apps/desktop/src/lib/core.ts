// Typed bridge to the in-process Rust core (Tauri commands + channels).
// Types mirror `locaryn-shared-types` and `locaryn-events` (serde snake_case).
//
// When the app runs outside Tauri (plain `vite dev` in a browser), a demo
// implementation with canned data takes over so the UI can be designed and
// tested without the Rust shell. The active mode is exposed as `coreMode`.

import { Channel, invoke } from "@tauri-apps/api/core";

export type TrustLevel = "trusted" | "untrusted" | "sandbox";
export type ConnectionMode = "auto" | "remote" | "local";
export type MessageRole = "user" | "assistant" | "tool" | "system";

export interface Project {
  id: string;
  path: string;
  name: string;
  trust_level: TrustLevel;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface Session {
  id: string;
  project_id: string;
  title: string | null;
  provider_id: string | null;
  model: string | null;
  created_at: string;
  last_message_at: string | null;
  closed_at: string | null;
  /** Rangée aux archives : absente des listes, mais pas perdue. */
  archived_at?: string | null;
  /** Éphémère : rien n'en sera gardé, pas même un titre. */
  ephemeral?: boolean;
  /** Noyau choisi pour cette conversation (id de l'extension de noyau).
   *  Absent ou null = noyau Locaryn natif. */
  core_id?: string | null;
}

export interface Message {
  id: string;
  session_id: string;
  role: MessageRole;
  content: string;
  tool_calls: { call_id: string; tool: string; args: unknown }[] | null;
  tool_call_id: string | null;
  tokens_in: number;
  tokens_out: number;
  parent_id: string | null;
  created_at: string;
}

export interface ProviderSummary {
  kind: "remote" | "local";
  engine: string;
  endpoint: string;
  model: string | null;
}

export type ProviderStatus = "unknown" | "healthy" | "unhealthy" | "starting";

export interface Provider {
  id: string;
  kind: "remote" | "local";
  engine: string;
  endpoint: string;
  model: string | null;
  is_active: boolean;
  status: ProviderStatus;
  config: unknown;
  created_at: string;
  updated_at: string;
}

// ── Fournisseurs distants apportés par une extension ───────────────────────
// Un morph peut déclarer un catalogue de modèles servi par une API compatible
// OpenAI — OpenRouter, et tout ce qui parle le même dialecte. Rien ne tourne
// sur la machine : l'extension déclare où appeler, l'hôte garde la clé dans le
// trousseau du système et l'ajoute lui-même aux requêtes. Le panneau de
// l'extension ne relit jamais la clé, il apprend seulement qu'elle existe.

/** Un catalogue distant, tel que l'interface le montre. */
export interface CloudProvider {
  /** Identifiant stable (« openrouter »). */
  id: string;
  label: string;
  /** L'extension qui l'apporte : la retirer retire le dossier. */
  extension_id: string;
  extension_name: string;
  /** Base de l'API, sans « /v1 ». */
  api_url: string;
  models_url: string;
  keys_url: string | null;
  docs_url: string | null;
  key_hint: string | null;
  /** Une clé est enregistrée — jamais la clé elle-même. */
  has_key: boolean;
  /** Modèles dans le catalogue gardé sur disque. */
  model_count: number;
  updated_at: string | null;
  /** Le modèle de ce catalogue actuellement actif, s'il l'est. */
  active_model: string | null;
  /** Vrai quand la passerelle tourne sur la machine (OmniRoute et consorts). */
  is_local: boolean;
  /** Le tableau de bord de la passerelle, quand elle en a un. */
  dashboard_url: string | null;
  /** Comment l'installer, dit en une phrase quand elle ne répond pas. */
  install_hint: string | null;
  /** Une commande de démarrage est déclarée dans le manifeste. */
  can_start: boolean;
  /** L'application sait-elle l'installer elle-même ? */
  can_install: boolean;
  /** Le programme est-il déjà présent sur la machine ? */
  installed: boolean;
}

/** L'état d'une passerelle locale. */
export interface CloudProviderStatus {
  running: boolean;
  /** Le programme est-il présent sur la machine ? */
  installed: boolean;
  /** Ce qu'il faut faire quand elle ne répond pas — jamais un code. */
  detail: string;
  dashboard_url: string | null;
}

/** Un modèle du catalogue distant. */
export interface CloudModel {
  id: string;
  name: string;
  description: string;
  context_length: number;
  /** Prix par million de jetons, en dollars. `null` si non publié. */
  prompt_price_per_m: number | null;
  completion_price_per_m: number | null;
  /** « text », « text+image->text »… tel que le catalogue le déclare. */
  modality: string;
  /** Sans appel d'outils, la boucle d'outils de Locaryn tourne à vide. */
  supports_tools: boolean;
}

export interface Health {
  status: string;
  version: string;
  mode: ConnectionMode;
  active_provider: ProviderSummary | null;
}

export interface AppInfo {
  version: string;
  mode: ConnectionMode;
  data_dir: string;
  db_path: string;
  /** Real weights directory — never hardcode this in the UI. */
  models_dir: string;
  /** OS of the running build: "windows", "macos" or "linux". */
  platform: string;
  /** CPU architecture, e.g. "x86_64", "aarch64", "x86". */
  arch: string;
}

/** Persistent identity of the local account shown in the desktop profile. */
export interface LocalProfile {
  display_name: string;
  /** Copied into Locaryn's data directory; null means initials are shown. */
  avatar_path: string | null;
}

// ── Storage location ───────────────────────────────────────────────────
// Weights and engines run to tens of gigabytes, so where they live is a
// user setting rather than a fixed path under the home directory.

export interface StorageEntry {
  key: string;
  label: string;
  path: string;
  size_bytes: number;
  exists: boolean;
  /** Sits outside the configured root — i.e. still filling another drive. */
  outside_root: boolean;
}

export interface DriveInfo {
  mount: string;
  total_bytes: number;
  free_bytes: number;
  is_current: boolean;
}

export interface StorageInfo {
  root: string;
  /** False while the built-in default is in use (user never chose). */
  configured: boolean;
  entries: StorageEntry[];
  total_bytes: number;
  drives: DriveInfo[];
  /** The live database. Stays put when the root moves — relocating an open
   *  SQLite file is how databases get corrupted. */
  db_path: string;
  db_bytes: number;
}

/** Emitted on the `storage-migration` Tauri event while data is relocated. */
export interface MigrationProgress {
  phase: string;
  current_file: string;
  moved_bytes: number;
  total_bytes: number;
  done: boolean;
  error: string | null;
}

/** Human-readable byte count. Shared so every storage view agrees. */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 o";
  const units = ["o", "Ko", "Mo", "Go", "To"];
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  const v = n / 1024 ** i;
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

// ── Tool approval (risk-based, doc 11 §5) ──────────────────────────────
// Mirrors `locaryn_events::Risk` and `locaryn_shared_types::{RiskScope,
// ApprovalVerdict, ToolApprovalDecision}`. The two enums stay byte-for-byte
// in sync with the Rust cargo workspace; divergence deserialises to a
// malformed `StreamEvent`.
export type RiskLevel = "low" | "medium" | "high" | "critical";
export type RiskScope = "once" | "session" | "project" | "always";
export type ApprovalVerdict = "allow" | "deny";

/** Payload streamed over SSE when the runtime needs consent. */
export interface ToolApprovalRequest {
  call_id: string;
  tool: string;
  args: unknown;
  risk: RiskLevel;
  /** Why the agent is asking — rendered in the modal's "Why" panel. */
  reason: string;
  /** Unified-diff head/tail (write_file), command preview (run_command),
   *  server+cmd (ssh_run_command). Null for read-only tools. */
  diff: string | null;
  /** True if the call crosses the machine boundary. Drives the banner. */
  is_remote: boolean;
}

/** What the frontend sends back to `approve_tool_call` IPC. */
export interface ToolApprovalDecision {
  call_id: string;
  tool: string;
  risk: RiskLevel;
  decision: ApprovalVerdict;
  scope: RiskScope;
  /** Persisted alongside the decision for the audit log. */
  note: string | null;
}

export interface ConnectorType {
  type_id: string;
  display_name: string;
  summary: string;
  icon: string;
  category: string; // "connector" | "extension" | "plugin"
  source: string; // "built-in" | "community"
  available: boolean;
  supports_test: boolean;
  /** Real command that runs this MCP server (empty for built-ins). */
  install_hint: string;
}

// ============================================================================
// Extensions
// ============================================================================

/** Where a bundle came from. Product names are never translated. */
export type ExtensionEcosystem =
  | "locaryn"
  | "claude_code"
  | "gemini_cli"
  | "opencode"
  | "mcp"
  | "cursor"
  | "continue"
  | "cline";

export const ECOSYSTEM_LABELS: Record<ExtensionEcosystem, string> = {
  locaryn: "Locaryn",
  claude_code: "Claude Code",
  gemini_cli: "Gemini CLI",
  opencode: "OpenCode",
  mcp: "MCP",
  cursor: "Cursor",
  continue: "Continue",
  cline: "Cline",
};

export type ExtensionPermission =
  | "shell"
  | "files_read"
  | "files_write"
  | "network"
  | "extensions"
  | "mcp"
  | "preview"
  | "lsp"
  | "env";

/** What each permission means, in the words shown in the approval modal. */
export const PERMISSION_LABELS: Record<ExtensionPermission, string> = {
  shell: "Exécuter des commandes",
  files_read: "Lire les fichiers du projet",
  files_write: "Modifier les fichiers du projet",
  network: "Accéder au réseau",
  extensions: "Gérer d'autres extensions",
  mcp: "Lancer ses serveurs MCP",
  preview: "Ouvrir des aperçus",
  lsp: "Enregistrer des serveurs LSP",
  env: "Lire des variables d'environnement",
};

export interface ExtensionComponents {
  skills: number;
  commands: number;
  agents: number;
  rules: number;
  hooks: number;
  mcp_servers: number;
  lsp_adapters: number;
  /** Figures apportées par le paquet (`figures/*.md`). */
  figures: number;
}

/** Un poids présent sur le disque, avec la place qu'il occupe. */
export interface StoredWeight {
  /** Chemin relatif au dossier de modèles. */
  name: string;
  size_bytes: number;
}

export interface ExtensionPermissionState {
  permission: ExtensionPermission;
  /** The plugin's own justification. Shown verbatim — it is not ours to edit. */
  reason: string | null;
  granted: boolean;
  /** Vrai tant que l'utilisateur n'a jamais tranché : ni accordée ni refusée.
   *  Une extension active dans cet état ne démarre pas son serveur MCP, et
   *  rien à l'écran ne le dit — d'où la distinction. */
  undecided?: boolean;
}

/** Une entrée d'interface apportée par une extension. */
/** Un bouton posé à côté du champ de saisie par une extension. */
export interface ExtensionComposerAction {
  id: string;
  label: string;
  icon?: string | null;
  /** `insert` écrit `value` dans le champ ; `tool` appelle l'outil nommé. */
  action: "insert" | "tool";
  value: string;
  hint?: string | null;
}

/** Une section de réglages apportée par une extension. */
export interface ExtensionSettingsSection {
  id: string;
  title: string;
  description?: string | null;
  fields: ExtensionSettingsField[];
}

export interface ExtensionSettingsField {
  key: string;
  label: string;
  /** `model`, `text`, `toggle` ou `choice`. */
  kind: string;
  hint?: string | null;
  options?: string[];
  default?: string | null;
}

export interface ExtensionUiEntry {
  id: string;
  label: string;
  icon: string | null;
}

export interface ExtensionUiSlotContribution {
  id: string;
  slot: string;
  order?: number;
  type?: "button" | "action" | "custom-element" | "iframe" | "modal" | string;
  label?: string;
  icon?: string | null;
  hint?: string | null;
  action?: "insert" | "tool" | "event" | "view" | "script" | string;
  value?: string | null;
  entry?: string | null;
  tag?: string | null;
  category?: string | null;
  /** Surfaces où cette contribution existe : "desktop", "mobile", "web".
   *  Absente ou vide : partout. Une extension déclare deux contributions au
   *  même slot, chacune ciblant sa surface, pour donner deux formes du même
   *  écran — un panneau large ici, autre chose sur le téléphone. */
  platforms?: string[];
}

export interface ExtensionUi {
  slots?: ExtensionUiSlotContribution[];
  nav_items: ExtensionUiEntry[];
  studio_tabs: ExtensionUiEntry[];
  /** Boutons près du champ de saisie. Vide quand l'extension est éteinte. */
  composer_actions?: ExtensionComposerAction[];
  /** Sections ajoutées aux réglages. Vide quand l'extension est éteinte. */
  settings_sections?: ExtensionSettingsSection[];
}

/** Les quatre groupes fixes de l'écran de mémoire. */
export type MemoryGroup = "vous" | "sujets" | "zones" | "personnes";

/** Une fiche que Locaryn retient de la personne — un sujet, pas une phrase. */
export interface MemoryEntry {
  id: string;
  user_id: string | null;
  group: MemoryGroup;
  title: string;
  /** Une ligne, montrée sans ouvrir la fiche. */
  summary: string;
  /** S'accumule au fil des conversations. */
  details: string[];
  /** `utilisateur` ou `assistant` : ce que le modèle a retenu se relit d'un autre œil. */
  source: string;
  created_at: string;
  updated_at: string;
}

/** Ce qu'a fait la boîte de commande, après avoir traduit une instruction en
 *  actions sur des fiches existantes. */
export interface MemoryCommandResult {
  summary: string;
  applied: number;
  entries: MemoryEntry[];
}

/**
 * Vitesse mesurée d'un modèle, sur cette machine.
 *
 * Les chiffres d'un catalogue viennent du matériel de celui qui les a publiés.
 * Ceux-ci viennent des générations réellement faites ici : c'est ce qui permet
 * de comparer deux modèles sur ce qu'ils donneront, pas sur ce qu'ils
 * promettent.
 */
export interface ModelMetric {
  model: string;
  /** `chat`, `image` ou `audio`. */
  kind: string;
  /** Nombre de générations derrière la moyenne. */
  samples: number;
  avg_tokens_per_second: number | null;
  avg_duration_ms: number | null;
  last_measured_at: string;
}

/** Ce qu'une extension de noyau déclare (section `core` du manifeste). */
export interface ExtensionCoreInfo {
  /** Dialecte piloté : `responses`, `runs`, `chat_completions`. */
  driver: string;
  /** URL de base de l'API du noyau (loopback). */
  api_url: string;
  port: number;
  /** Modèle annoncé par défaut (ex. `hermes-agent`). */
  model?: string | null;
  /** Chemin de l'index de skills, relatif au dossier de l'extension. */
  skills_index?: string | null;
  /** Commande d'installation d'un skill, avec `{{slug}}` à remplacer. */
  skills_install?: string | null;
}

/** État courant d'un noyau, pour sa carte dans les réglages. */
export type CoreState = "stopped" | "starting" | "running" | "external" | "error";

export interface CoreStatus {
  id: string;
  state: CoreState;
  driver: string;
  api_url: string;
  error?: string | null;
}

/** Un skill de l'index déclaré par le noyau. */
export interface CoreSkillEntry {
  slug: string;
  name: string;
  description?: string | null;
  verified: boolean;
}

export interface InstalledExtension {
  id: string;
  name: string;
  display_name: string;
  version: string;
  api_version: string;
  description: string | null;
  author: string | null;
  homepage: string | null;
  kind: string;
  scope: string;
  ecosystem: ExtensionEcosystem;
  source: string | null;
  install_dir: string;
  enabled: boolean;
  components: ExtensionComponents;
  /**
   * Ce que l'extension sait faire : `image-gen`, `voice-tts`, `model-training`…
   * Vide quand elle est désactivée. La navigation s'en sert pour décider quels
   * écrans existent.
   */
  capabilities: string[];
  /** Ce qu'elle ajoute à l'interface. */
  ui: ExtensionUi;
  permissions: ExtensionPermissionState[];
  /** Components that failed to parse. The plugin still runs without them. */
  load_errors: string[];
  /** Section `core` du manifeste — présent = cette extension est un noyau. */
  core?: ExtensionCoreInfo | null;
  created_at: string;
  updated_at: string;
}

/** How much of a catalog entry can actually run here. */
export type CatalogCompat = "native" | "adapted" | "partial" | "unsupported";

export interface MorphVersionRelease {
  version: string;
  tag?: string;
  is_beta: boolean;
  released_at?: string;
  summary?: string;
  install_source?: string;
}

export interface CatalogEntry {
  id: string;
  name: string;
  display_name: string;
  description: string | null;
  author: string | null;
  version: string | null;
  homepage: string | null;
  ecosystem: ExtensionEcosystem;
  catalog_id: string;
  catalog_label: string;
  /** What to pass to `installExtension`. Empty when not installable. */
  install_source: string;
  keywords: string[];
  /** What the catalog claims it ships. Advertised, not verified. */
  advertised: string[];
  compat: CatalogCompat;
  installed: boolean;
  is_beta?: boolean;
  versions?: MorphVersionRelease[];
}

export interface CatalogSource {
  id: string;
  label: string;
  ecosystem: ExtensionEcosystem;
  url: string;
  builtin: boolean;
  enabled: boolean;
}

export interface CatalogSourceStatus {
  source: CatalogSource;
  ok: boolean;
  entry_count: number;
  error: string | null;
}

export interface CatalogSnapshot {
  entries: CatalogEntry[];
  sources: CatalogSourceStatus[];
  fetched_at: string | null;
  /** Served from cache because every source failed. */
  stale: boolean;
}

export interface ExtensionCommand {
  name: string;
  plugin: string;
  description: string | null;
  arguments: string[];
}

/**
 * Un champ de réglage déclaré par une extension. L'application ne connaît que
 * ces types génériques : elle ne sait rien de ce que tel ou tel plugin demande.
 */
export type ExtensionFieldType =
  | "string"
  | "text"
  | "number"
  | "boolean"
  | "select"
  | "list"
  | "path"
  | "secret"
  | "model"
  | "prompt";

export interface ExtensionField {
  type: ExtensionFieldType;
  title?: string;
  description?: string;
  default?: unknown;
  /** Valeurs acceptées pour un champ `select`. */
  options?: string[];
  /** Libellés affichés en face de `options`, même ordre. */
  optionLabels?: string[];
  min?: number;
  max?: number;
  step?: number;
  /** Section du formulaire. Les champs sans groupe passent en « Général ». */
  group?: string;
}

/** Le formulaire d'une extension : ce qu'elle demande, et ce qui est enregistré. */
export interface ExtensionConfig {
  /** `null` quand l'extension ne déclare aucun réglage. */
  schema: Record<string, ExtensionField> | null;
  values: Record<string, unknown>;
}

/** Un serveur MCP déclaré par une extension, éditable dans le panneau Réglages.
 *  Seuls `env` et `auto_start` sont modifiables ; la commande/URL et le
 *  transport viennent du fichier du plugin et restent intacts. */
export interface ExtensionMcpServer {
  name: string;
  transport: "stdio" | "http";
  /** Ligne de commande (stdio) ou URL (http), affichée telle quelle. */
  target: string;
  env: Record<string, string>;
  auto_start: boolean;
}

/** Résultat du contrôle de version d'une extension (source GitHub). */
export interface ExtensionUpdateCheck {
  id: string;
  /** Dernière version sur la branche par défaut, quand la source est vérifiable. */
  latest_version: string | null;
  /** Vrai quand une version plus récente existe. */
  update_available: boolean;
  /** Pourquoi le contrôle n'a pas pu se faire (réseau, forme de source…). */
  error: string | null;
}

/** Aperçu d'une source d'installation : ce que le manifeste déclare, sans
 *  télécharger le paquet. Les permissions sont telles que déclarées par la
 *  source (pas forcément dans le vocabulaire Locaryn). */
export interface ExtensionSourcePreview {
  /** morph.json, .claude-plugin/plugin.json, … */
  manifest_file: string;
  /** locaryn, claude_code, gemini_cli, opencode, mcp */
  ecosystem: string;
  name: string;
  version: string | null;
  description: string | null;
  author: string | null;
  requested_permissions: string[];
  /** Serveurs MCP déclarés par la source (lien nom → commande ou URL). */
  mcp_servers: McpServerPreview[];
}

/** Serveur MCP déclaré par la source, affiché avant l'installation. */
export interface McpServerPreview {
  name: string;
  /** Commande stdio (npx …) quand le transport est stdio. */
  command: string | null;
  /** URL du serveur quand le transport est distant. */
  url: string | null;
}

export interface Bootstrap {
  project: Project;
  session: Session;
  health: Health;
}

export interface ModelParams {
  temperature: number;
  top_p: number;
  top_k: number;
  ctx_size: number;
  max_tokens: number;
  repeat_penalty: number;
  seed: number;
}

/** Sampling and cloning-style controls for Qwen3-TTS.
 *
 *  Defaults match the engine's own. The values the app used to hardcode
 *  (temperature 0.7, timbre-only cloning) produced a flat, robotic delivery. */
export interface TtsSampling {
  /** Higher = more varied intonation. Engine default 0.9. */
  temperature: number;
  topK: number;
  topP: number;
  repetitionPenalty: number;
  /** In-context cloning: condition on the reference audio *and* its
   *  transcript so the speaker's rhythm carries over. Off = timbre only,
   *  which reads the sentence flatly. */
  expressive: boolean;
  /** Transcript of the reference clip. Transcribed automatically if empty. */
  referenceText: string;
  /** Silence stretch applied after rendering: >1 = more measured delivery.
   *  Post-processing, so it behaves identically on every engine. */
  pauseScale: number;
  /** Pitch shift of the rendered speech. Neutral at 1.0. */
  pitch: number;
  /** Presence and evenness. Neutral at 0.7 so old presets are unchanged. */
  energy: number;
  /** Consonant crispness (2-5 kHz band). Neutral at 0.8. */
  clarity: number;
}

export const TTS_SAMPLING_DEFAULTS: TtsSampling = {
  temperature: 0.9,
  topK: 50,
  topP: 1.0,
  repetitionPenalty: 1.05,
  expressive: true,
  referenceText: "",
  pauseScale: 1.0,
  pitch: 1.0,
  energy: 0.7,
  clarity: 0.8,
};

// ── Voice presets ──────────────────────────────────────────────────────
// A saved voice: the reference recording plus how to speak with it, so a
// voice can be reused without re-uploading a sample and re-tuning sliders.

export interface VoiceSettings {
  speed: number;
  pitch: number;
  energy: number;
  clarity: number;
  /** Silence stretch. >1 = more measured. Post-processing, works everywhere. */
  pauseScale: number;
  temperature: number;
  topK: number;
  topP: number;
  repetitionPenalty: number;
  /** In-context cloning: reproduce the speaker's rhythm, not only timbre. */
  expressive: boolean;
  instruct: string;
}

export const VOICE_SETTINGS_DEFAULTS: VoiceSettings = {
  speed: 1.0,
  pitch: 1.0,
  energy: 0.7,
  clarity: 0.8,
  pauseScale: 1.0,
  temperature: 0.9,
  topK: 50,
  topP: 1.0,
  repetitionPenalty: 1.05,
  expressive: true,
  instruct: "",
};

export interface VoicePreset {
  id: string;
  name: string;
  note: string;
  /** Absolute path to the copied reference recording. */
  referenceAudio: string;
  referenceText: string;
  language: string;
  durationS: number;
  settings: VoiceSettings;
  engine: string;
  createdAt: string;
  updatedAt: string;
}

/** What a given model actually honours from a preset. */
export interface EngineSupport {
  engine: string;
  cloning: boolean;
  referenceText: boolean;
  temperature: boolean;
  speed: boolean;
  pitch: boolean;
  pauseScale: boolean;
  instruct: boolean;
}

export interface SavePresetArgs {
  id?: string;
  name: string;
  note?: string;
  referenceAudio?: string;
  referenceText?: string;
  language?: string;
  engine?: string;
  settings: VoiceSettings;
}

// ── Server mode ────────────────────────────────────────────────────────
// The app supervises the Locaryn service rather than serving HTTP itself, so
// the accounts, tokens and encryption all live in one implementation.

export interface ServerStatus {
  running: boolean;
  bind: string;
  port: number;
  /** Address to hand to other machines. Empty while stopped. */
  url: string;
  accounts: number;
  fingerprint: string | null;
  /** Why it cannot start right now, if it cannot. */
  blocker: string | null;
}

export interface ServerUserSummary {
  id: string;
  username: string;
  role: string;
  disabled: boolean;
}

/** Settings an administrator prepared for this machine. */
export interface Provisioning {
  serverUrl: string;
  organisation: string;
  certificateFingerprint: string | null;
  note: string;
}

/**
 * Par où un téléphone joint cette machine.
 *
 * `local` : le réseau de la maison ou du bureau — rien à ouvrir, rien à
 * traverser. `tunnel` : la machine appelle un relais, donc rien n'est ouvert
 * sur la box non plus. `public` : un port a été redirigé, ou la machine a une
 * adresse fixe — c'est le seul cas où quelque chose est exposé.
 */
export type PairingMode = "local" | "tunnel" | "public";

/**
 * Le modèle des micro-tâches.
 *
 * Nommer une conversation, ranger, résumer : des travaux courts qui n'ont pas
 * besoin du gros modèle. Aucun n'est choisi par défaut — tant qu'il n'y en a
 * pas, ces services ne tournent pas.
 */
/** Le caractère donné au modèle, s'il lui en a été donné un. */
export interface SystemPrompt {
  /** `null` : rien n'est posé devant le modèle — le cas par défaut. */
  texte: string | null;
  /** Le message système exact qu'une conversation avec outils enverra. */
  envoye: string;
}

export interface MicroModel {
  model: string | null;
  available: string[];
}

/** Un code d'appairage et l'adresse qu'il porte. */
export interface PairingCode {
  mode: string;
  url: string;
  qr_svg: string;
}

/**
 * Une figure : un rôle et un agencement à la fois.
 *
 * Ses consignes sont versées au prompt système de chacune de ses
 * conversations, devant la mémoire de l'utilisateur — le rôle qu'on lui a
 * donné prime sur ce que le service sait par ailleurs.
 */
export interface Figure {
  id: string;
  name: string;
  description: string;
  instructions: string;
  model: string | null;
  opening: string | null;
  /** Fausse : la figure travaille sans rien savoir de son utilisateur. */
  uses_memory: boolean;
  /** Les outils qu'elle a le droit d'appeler. Vide : tout ce que l'application propose. */
  tools: string[] | null;
  /** `user` quand elle est écrite à la main ; sinon le dépôt d'où elle vient. */
  source: string;
  created_at: string;
  updated_at: string;
}

/** Ce qu'on envoie pour créer ou corriger une figure. */
export interface FigureDraft {
  name: string;
  description: string;
  instructions: string;
  model: string | null;
  opening: string | null;
  usesMemory: boolean;
  /** Les outils autorisés, séparés par des virgules. Vide : tout. */
  tools: string;
}

/** Travel mode: this machine reachable from elsewhere, through a relay. */
export interface TravelStatus {
  active: boolean;
  provider: string | null;
  /** The signed pairing link. Never a network address. */
  link: string | null;
  /** The same link drawn as an inline SVG. */
  qr_svg: string | null;
  blocker: string | null;
}

/** One relay, and its state on this machine. */
export interface RelayChoice {
  id: string;
  label: string;
  installed: boolean;
  needs_account: boolean;
  install_hint: string;
}

/** An MCP server registered in `mcp.json`. */
export interface McpServerInfo {
  name: string;
  transport: "stdio" | "http";
  /** The command line or the URL, as it was typed. */
  target: string;
  running: boolean;
  auto_start: boolean;
  /** Tools the server announced, once started. */
  tools: string[];
}

export interface AddMcpServerArgs {
  name: string;
  transport: "stdio" | "http";
  target: string;
  env?: Record<string, string>;
  autoStart?: boolean;
}

export interface AndroidVmStatus {
  sdkRoot: string | null;
  sdkmanager: string | null;
  avdmanager: string | null;
  emulator: string | null;
  avds: string[];
  runningEmulators: string[];
  recommendedAvd: string;
  detail: string;
}

export interface AndroidVmSetupArgs {
  avdName?: string;
  apiLevel?: number;
  installComponents?: boolean;
}

export interface AndroidScreenProbe {
  serial: string;
  state: string;
  bootCompleted: boolean;
  displaySize: string | null;
  screenshotBase64: string;
  uiXml: string;
  uiText: string[];
  ocrText: string | null;
  ocrAvailable: boolean;
  ocrDetail: string;
}

export interface AndroidScreenArgs {
  serial?: string;
  ocr?: boolean;
}

export interface AndroidScreenActionArgs extends AndroidScreenArgs {
  action: "tap" | "swipe" | "back" | "home" | "refresh";
  x?: number;
  y?: number;
  x2?: number;
  y2?: number;
  durationMs?: number;
}

/** The client certificate registered with this installation, if any. */
export interface CertificateStatus {
  installed: boolean;
  /** Name the certificate was issued to, read from the file itself. */
  issued_to: string | null;
  path: string | null;
  /** True once the deployment authority is present too, which lets the client
   *  check the server in return. */
  authority_installed: boolean;
}

/** A signed-in session against a remote Locaryn server. */
export interface ServerSession {
  server_url: string;
  username: string;
  token: string;
}

/** Result of an audio / TTS generation. */
export interface GeneratedAudio {
  path: string;
  simulated: boolean;
}

/** Verdict on whether a plain chat message is really an image request. */
export interface ImageIntent {
  is_image: boolean;
  is_edit: boolean;
  english_prompt: string;
  quality: string;
  reason: string;
}

export interface ImageDefaults {
  quality: string;
  width: number;
  height: number;
  steps: number;
  cfg_scale: number;
  vram_mode: string;
  negative_prompt: string;
  variants: number;
}

/** A plan produced by the model for a substantial request. */
export interface TaskPlan {
  needs_plan: boolean;
  /** Verify the result and replay the plan on failure (bug fixes). */
  needs_loop: boolean;
  steps: string[];
}

/** Defaults for model-backed features outside the main conversation. */
export interface ModelPreferences {
  /** Null means the Studio chooses the first installed TTS model. */
  tts_model: string | null;
  /** Null means the first installed image diffusion model. */
  image_model?: string | null;
}

/** One complete model choice inside a HuggingFace repository. */
export interface HfModelCandidate {
  id: string;
  label: string;
  files: string[];
  /** Candidate-specific runtime companions such as a multimodal projector. */
  support_files: string[];
  total_bytes: number;
  format: string;
  quantization: string | null;
  variant: string | null;
}

export interface HfRepoInspection {
  repo: string;
  candidates: HfModelCandidate[];
  support_files: string[];
  total_bytes: number;
  warning: string | null;
  /** A known conversion that the managed llama.cpp runtime can actually load. */
  suggested_repo: string | null;
}

export interface HfModelSelection {
  repo: string;
  files: string[];
  support_files?: string[];
  label?: string | null;
}

/** State of the managed llama.cpp runtime. */
/**
 * Un moteur d'inférence : le runtime intégré, ou celui qu'une extension
 * apporte. `engine` est le jeton (`llama_cpp`, `ext:mon-moteur`) que les
 * commandes de démarrage et d'arrêt attendent.
 */
export interface InferenceEngineInfo {
  engine: string;
  label: string;
  /** Extension qui l'apporte, `null` pour un runtime intégré. */
  extension: string | null;
  extensionVersion: string | null;
  endpoint: string;
  healthy: boolean;
  /** Le processus a été lancé par l'application, donc elle peut l'arrêter. */
  owned: boolean;
  active: boolean;
  model: string | null;
  /** Formats de poids servis, en mots lisibles. */
  formats: string[];
  /** Ce qui manque sur cette machine, dans les mots de l'auteur du moteur. */
  unmetRequirement: string | null;
  logPath: string | null;
}

export interface LlamaRuntimeStatus {
  installed: boolean;
  version: string | null;
  up_to_date: boolean;
  pinned: string;
  path: string;
}

/** One honest snapshot of everything the local runtime can do. */
/** OpenAI-style response_format for structured output (llama-server supports both). */
export type ResponseFormat =
  | { type: "json_object" }
  | { type: "json_schema"; json_schema: { name: string; schema: unknown; strict?: boolean } };

/** Chain-of-thought regulation for reasoning-capable models (Qwen3, etc.). */
export type ReasoningLevel = "off" | "auto" | "low" | "medium" | "high" | "extreme";

/** Map a level to the request fields llama-server understands. Safe no-op on
 *  non-thinking models. `off` disables thinking; the rest set a token budget
 *  (-1 = unlimited). `auto` leaves the model's default untouched. */
export function reasoningPayload(level: ReasoningLevel): Record<string, unknown> | null {
  switch (level) {
    case "off":
      return { reasoning_budget: 0, chat_template_kwargs: { enable_thinking: false } };
    case "low":
      return { reasoning_budget: 512, chat_template_kwargs: { enable_thinking: true } };
    case "medium":
      return { reasoning_budget: 2048, chat_template_kwargs: { enable_thinking: true } };
    case "high":
      return { reasoning_budget: 8192, chat_template_kwargs: { enable_thinking: true } };
    case "extreme":
      return { reasoning_budget: -1, chat_template_kwargs: { enable_thinking: true } };
    default:
      return null;
  }
}

export interface RuntimeCapabilities {
  runtime_installed: boolean;
  runtime_version: string | null;
  chat: boolean;
  /** Legacy capability bit retained for extension compatibility. */
  image_gen: boolean;
  vision: boolean;
  embeddings: boolean;
  finetune: boolean;
  distributed: boolean;
  speculative_decoding: boolean;
  kv_quant: boolean;
  weight_formats: string[];
  unavailable: string[];
}

// `StreamEvent` from locaryn-events — #[serde(tag = "type", snake_case)].
export type StreamEvent =
  | { type: "message_start"; message_id: string; task_id: string }
  | { type: "token"; text: string }
  | { type: "tool_call"; call_id: string; tool: string; args: unknown }
  | {
      type: "tool_approval";
      call_id: string;
      tool: string;
      args: unknown;
      risk: RiskLevel;
      reason: string;
      diff: string | null;
      is_remote: boolean;
    }
  | { type: "tool_result"; call_id: string; ok: boolean; output: string }
  | { type: "artifact"; artifact_id: string; kind: string; path: string }
  | { type: "task_update"; task_id: string; status: string; progress: number }
  | { type: "preview_update"; artifact_id: string; url: string }
  | {
      type: "provider_changed";
      provider: string;
      engine: string;
      model: string | null;
      reason: string;
    }
  | { type: "log"; level: string; msg: string; source: string }
  | {
      type: "message_end";
      message_id: string;
      tokens_in: number;
      tokens_out: number;
      duration_ms: number;
    };

export type TerminalEvent =
  | { type: "line"; stream: "stdout" | "stderr"; text: string }
  | { type: "exit"; code: number | null };

export interface HardwareSpec {
  total_ram_gb: number;
  total_vram_gb: number;
  recommended_size_label: string;
  cpu_cores?: number;
}

// ── Résidence du modèle de chat ────────────────────────────────────────
// `llama-server` garde les poids en mémoire tant qu'il tourne : parler au
// modèle ne le recharge pas. Mais sans épinglage, le superviseur décharge
// après un temps d'inactivité, et l'utilisateur repaie le chargement sans
// l'avoir demandé. Ces types donnent la main sur ce cycle.

/** Jusqu'où l'application accepte de remplir la mémoire avant de charger. */
export type CautionLevel = "prudent" | "equilibre" | "risque";

export const CAUTION_LABELS: Record<CautionLevel, { label: string; hint: string }> = {
  prudent: {
    label: "Sécurité",
    hint: "Ne charge que si la machine a largement de quoi. Refuse tout ce qui pourrait la ralentir.",
  },
  equilibre: {
    label: "Intermédiaire",
    hint: "Charge avec précautions : accepte que ce soit juste, refuse ce qui déborderait.",
  },
  risque: {
    label: "Risqué",
    hint: "Ne refuse jamais. Prévient, puis charge — au prix d'un ralentissement sévère, voire d'un plantage.",
  },
};

/** Confortable = tient sur le GPU ; juste = tient mais plus lent ; risqué =
 *  déborde sur le disque ; refusé = bloqué par le niveau de prudence. */
export type FitVerdict = "confortable" | "juste" | "risque" | "refuse";

export interface ModelFit {
  model: string;
  verdict: FitVerdict;
  /** Taille des poids. */
  size_gb: number;
  /** Cache d'attention pour le contexte réglé — il dépasse les poids sur les
   *  longs contextes, et c'est lui qu'on raccourcit en premier. */
  kv_cache_gb: number;
  /** Tampons de calcul et surcoût du moteur. */
  compute_gb: number;
  /** Ce qu'il faut réellement, marge de prudence comprise. */
  required_gb: number;
  free_ram_gb: number;
  free_vram_gb: number;
  /** "gpu" | "partage" | "ram" | "disque" — où les poids finiront. */
  placement: string;
  level: CautionLevel;
  /** Contexte pris en compte, en jetons. */
  context: number;
  /** Couches placées sur le GPU, sur le total. */
  gpu_layers: number;
  total_layers: number;
  /** Débit de génération estimé, en jetons par seconde. */
  tokens_per_second: number;
  /** Débit de lecture du prompt : plus élevé, et moins certain. */
  prompt_tokens_per_second: number;
  /** Le plus grand contexte qui tiendrait entièrement sur le GPU. */
  max_gpu_context: number;
  /** Le plus grand contexte qui tiendrait, GPU et RAM réunis. */
  max_context: number;
  /** Quantification du fichier. */
  quant: string;
  /** Une quantification plus légère qui, elle, tiendrait sur le GPU. */
  suggested_quant: string | null;
  /** Vrai quand les dimensions sont déduites et non lues dans le fichier. */
  estimated: boolean;
  /** Ce que ces chiffres supposent. Affiché tel quel, jamais résumé. */
  assumptions: string[];
  /** Peut-on forcer malgré le refus ? */
  overridable: boolean;
  /** Phrase montrée telle quelle : ce qui va se passer, pas un code. */
  message: string;
}

/** La machine telle que l'estimateur la mesure. */
export interface LlmfitHardware {
  cpu_cores: number;
  total_ram_gb: number;
  free_ram_gb: number;
  gpu_name: string | null;
  total_vram_gb: number;
  free_vram_gb: number;
  backend: "cuda" | "metal" | "rocm" | "vulkan" | "cpu";
  /** Bande passante mémoire système, en Go/s. */
  ram_bandwidth_gbps: number;
  /** Bande passante de la mémoire graphique, en Go/s. */
  vram_bandwidth_gbps: number;
  /** Vrai quand la bande passante vient d'une mesure sur cette machine. */
  ram_bandwidth_measured: boolean;
  /** Mémoire unifiée : la VRAM est la RAM. */
  unified_memory: boolean;
}

/** Une fiche de catalogue à estimer avant tout téléchargement. */
export interface LlmfitCatalogEntry {
  /** Identifiant stable, renvoyé tel quel pour l'appariement. */
  id: string;
  /** Paramètres du modèle, en milliards. */
  parameters_b: number;
  /** Étiquette de quantification (« Q4_K_M »), si elle est connue. */
  quant?: string;
  /** Taille annoncée du téléchargement, en Go. Prime sur la taille déduite. */
  size_gb?: number;
}

export interface ResidencyStatus {
  /** Le moteur tourne et répond. */
  loaded: boolean;
  model: string | null;
  /** Épinglé : aucun minuteur ne le déchargera. */
  pinned: boolean;
  idle_seconds: number;
  /** Au-delà, un modèle non épinglé est déchargé. */
  idle_timeout_seconds: number;
  endpoint: string | null;
}

export type KvCacheType = "f16" | "q8_0" | "q4_0";
export type InferenceProfile = "eco" | "balanced" | "performance" | "turbo" | "longctx" | "custom";

export interface InferenceConfig {
  /** Named preset base */
  profile: InferenceProfile;
  /** -1 = all layers on GPU, 0 = CPU only, N = exact count */
  gpu_layers: number;
  /** KV cache compression type */
  kv_cache_type: KvCacheType;
  /** Context window in tokens */
  context_length: number;
  /** Flash Attention (saves ~30% VRAM) */
  flash_attention: boolean;
  /** CPU threads, 0 = auto */
  cpu_threads: number;
  /** Batch size (higher = faster + more VRAM) */
  batch_size: number;
  /** Deprecated no-op (kept for config compatibility; ignored by the runtime). */
  use_turboquant: boolean;
  /** Draft model path for speculative decoding */
  draft_model_path: string;
  /** mmap model loading */
  use_mmap: boolean;
  /** Parallel inference slots */
  parallel_slots: number;
  /** MoE expert offload to CPU: 0 = off, -1 = all experts, N = first N layers. */
  n_cpu_moe: number;
  /** Distributed inference: comma-separated RPC workers (host:port). Empty = off. */
  rpc_servers: string;
  /** LoRA adapter .gguf files preloaded at server start (hot-swappable scales). */
  lora_adapters: string[];
}

export interface LoraAdapter {
  id: number;
  path: string;
  scale: number;
}

export interface LoraScale {
  id: number;
  scale: number;
}

/** How a model should be run on this machine (auto routing). */
export interface RuntimePlan {
  model: string;
  size_gb: number;
  vram_gb: number;
  ram_gb: number;
  /** "gpu" | "offload" | "heavy" */
  mode: string;
  /** Suffix shown next to the model name, e.g. "offload RAM". */
  label: string;
  gpu_layers: number;
  n_cpu_moe: number;
}

export interface RagSource {
  source: string;
  chunks: number;
}

export interface RagStatus {
  chunk_count: number;
  dim: number;
  embed_model: string;
  sources: RagSource[];
}

export interface RagHit {
  source: string;
  text: string;
  score: number;
}

/** Une capacité reconnue par le serveur : id, label français, description. */
export interface Capability {
  id: string;
  label: string;
  description: string;
}

export interface CoreApi {
  health(): Promise<Health>;
  bootstrap(): Promise<Bootstrap>;
  listProjects(): Promise<Project[]>;
  createProject(path: string, name: string, trustLevel?: TrustLevel): Promise<Project>;
  /** Rename a project and/or change its trust level (agent permissions). */
  updateProject(id: string, name?: string, trustLevel?: TrustLevel): Promise<Project>;
  /** Archive (soft-delete) a project: hidden from the sidebar, history kept. */
  archiveProject(id: string): Promise<void>;
  /** Hidden project owning project-less ("free") chats. */
  freeChatProject(): Promise<Project>;
  /** Workspace directory for a session (project path, or temp folder for free chats). */
  sessionWorkspace(sessionId: string): Promise<string>;
  /** A plan the model produced for a substantial request. */
  planTask(request: string): Promise<TaskPlan>;
  /** Legacy wire method used by older image extensions. */
  getImageDefaults(): Promise<ImageDefaults>;
  setImageDefaults(config: ImageDefaults): Promise<void>;
  /** Background call: 1-click next-step suggestions after an answer. */
  /** `question` est le message auquel la réponse répond : sans lui, les
   *  suggestions ne savent pas de quoi parle la conversation. */
  suggestFollowups(answer: string, question?: string): Promise<string[]>;
  /** Persist a message contributed by an extension without exposing its runtime. */
  appendChatMessage(sessionId: string, role: "user" | "assistant", content: string): Promise<void>;
  /** Legacy convenience for assistant artifacts contributed by extensions. */
  appendAssistantMessage(sessionId: string, content: string): Promise<void>;
  /** Defaults for secondary model-backed features in the account profile. */
  getModelPreferences(): Promise<ModelPreferences>;
  setModelPreferences(preferences: ModelPreferences): Promise<void>;
  /** How this machine should run a model (auto GPU / RAM-offload routing). */
  planModelRuntime(model: string): Promise<RuntimePlan>;
  listSessions(projectId: string): Promise<Session[]>;
  /** Create a chat; `title` auto-names it (e.g. from the first prompt). */
  /** `coreId` = id de l'extension de noyau (OpenClaw, Hermes…) ; absent =
   *  noyau Locaryn natif. */
  createSession(projectId: string, title?: string, coreId?: string | null): Promise<Session>;
  /** Rename a session. */
  updateSessionTitle(sessionId: string, title: string): Promise<void>;
  /** Ask the LLM to generate and persist a concise title for a session. */
  generateSessionTitle(sessionId: string, firstPrompt: string): Promise<string>;
  /** Permanently delete a session and its messages. */
  deleteSession(sessionId: string): Promise<void>;
  /** Ranger aux archives, ou en ressortir. Rien n'est perdu. */
  archiveSession(sessionId: string, archived: boolean): Promise<void>;
  /** Ce qui a été rangé, pour un projet. */
  archivedSessions(projectId: string): Promise<Session[]>;
  /** Appeler l'outil qu'un bouton d'extension désigne, avec le texte du champ. */
  runComposerTool(tool: string, text: string): Promise<string>;
  /** Déplacer une conversation dans un projet. */
  moveSession(sessionId: string, projectId: string): Promise<void>;
  /** Où le petit modèle rangerait cette conversation. Presque toujours nulle
   *  part : une proposition à côté agace plus qu'elle n'aide. */
  suggestProject(
    sessionId: string,
  ): Promise<{ project_id?: string | null; project_name?: string | null }>;
  /** Verser une conversation dans une autre. Le petit modèle en écrit un seul
   *  récit ; celle qui a été déposée part aux archives, jamais à la poubelle. */
  mergeSessions(sessionId: string, sourceId: string): Promise<void>;
  /** Renommer à la main : le titre devient définitif. */
  renameSession(sessionId: string, title: string): Promise<void>;
  /** Une conversation dont rien ne sera gardé. */
  createEphemeralSession(projectId: string): Promise<Session>;

  /** Les figures : un rôle, ses consignes, ses conversations. */
  listFigures(): Promise<Figure[]>;
  saveFigure(f: FigureDraft): Promise<Figure>;
  deleteFigure(id: string): Promise<void>;
  attachFigure(sessionId: string, figureId: string | null): Promise<void>;
  figureSessions(figureId: string): Promise<Session[]>;
  listMessages(sessionId: string): Promise<Message[]>;
  sendMessage(
    sessionId: string,
    content: string,
    onEvent: (ev: StreamEvent) => void,
    images?: string[],
    responseFormat?: ResponseFormat | null,
    reasoning?: Record<string, unknown> | null,
  ): Promise<void>;
  runTerminal(
    command: string,
    cwd: string | null,
    onOutput: (ev: TerminalEvent) => void,
  ): Promise<void>;
  listProviders(): Promise<Provider[]>;
  setActiveProvider(id: string): Promise<Provider>;
  configureProvider(endpoint: string, model: string | null): Promise<Provider>;
  listModels(endpoint: string): Promise<string[]>;
  /** Transformers repos present on disk but not loadable by llama.cpp. */
  listIncompatibleModels(): Promise<string[]>;
  /** Poids stockés que le moteur de conversation ne charge pas. L'hôte ne sait
   *  pas à quoi ils servent ; une extension les revendique par son catalogue. */
  listNonChatModels(): Promise<StoredWeight[]>;
  appInfo(): Promise<AppInfo>;
  getLocalProfile(): Promise<LocalProfile>;
  setLocalProfile(displayName: string): Promise<LocalProfile>;
  setLocalAvatar(sourcePath: string): Promise<LocalProfile>;
  clearLocalAvatar(): Promise<LocalProfile>;

  // --- AirLLM (low-VRAM inference engine) --------------------------------
  airllmStatus(): Promise<{
    python: boolean;
    pythonPath?: string | null;
    torch: boolean;
    airllmInstalled: boolean;
    installed: { repo: string; installedAt: string }[];
  }>;
  airllmSetup(onLine?: (text: string) => void): Promise<void>;
  airllmInstall(
    repo: string,
    onLine?: (text: string) => void,
  ): Promise<{ repo: string; installedAt: string }>;
  airllmInstalled(): Promise<{ repo: string; installedAt: string }[]>;
  airllmUninstall(repo: string): Promise<void>;
  configureAirllmProvider(repo: string): Promise<Provider>;

  listVoicePresets(): Promise<VoicePreset[]>;
  saveVoicePreset(args: SavePresetArgs): Promise<VoicePreset>;
  deleteVoicePreset(id: string): Promise<void>;
  /** Which preset settings the given model will actually use. */
  voicePresetSupport(model: string): Promise<EngineSupport>;
  serverStatus(): Promise<ServerStatus>;
  setServerMode(enabled: boolean, port?: number): Promise<ServerStatus>;
  restartServer(): Promise<ServerStatus>;
  listServerUsers(): Promise<ServerUserSummary[]>;
  createServerUser(username: string, password: string, isAdmin?: boolean): Promise<ServerStatus>;
  deleteServerUser(userId: string): Promise<ServerStatus>;
  /** Deployment settings dropped next to the installer, if any. */
  provisioning(): Promise<Provisioning | null>;

  /** Exchange credentials for a token, and remember it. */
  signIn(serverUrl: string, username: string, password: string): Promise<ServerSession>;
  /** The stored session, if this machine already signed in. */
  currentSession(): Promise<ServerSession | null>;
  signOut(): Promise<void>;

  clientCertificateStatus(): Promise<CertificateStatus>;
  /** Register a certificate an administrator issued. `authority` is the CA
   *  file, needed only when the server does not use a public authority. */
  installClientCertificate(source: string, authority?: string): Promise<CertificateStatus>;
  removeClientCertificate(): Promise<CertificateStatus>;

  storageInfo(): Promise<StorageInfo>;
  /** Point Locaryn at `newRoot`, optionally relocating the existing data.
   *  Progress arrives on the `storage-migration` event. */
  setStorageRoot(newRoot: string, moveData: boolean): Promise<StorageInfo>;
  /** Delete scratch files. Resolves with the number of bytes reclaimed. */
  cleanTemp(): Promise<number>;

  listConnectorTypes(): Promise<ConnectorType[]>;

  // --- Extensions ---------------------------------------------------------
  listModelMetrics(): Promise<ModelMetric[]>;
  listMemory(): Promise<MemoryEntry[]>;
  /** Retenir un détail à la main. `group` retombe sur « sujets » sinon. */
  remember(group: MemoryGroup, title: string, detail: string): Promise<MemoryEntry>;
  setMemorySummary(id: string, summary: string): Promise<MemoryEntry>;
  renameMemoryEntry(id: string, title: string): Promise<MemoryEntry>;
  setMemoryGroup(id: string, group: MemoryGroup): Promise<MemoryEntry>;
  removeMemoryDetail(id: string, detail: string): Promise<MemoryEntry>;
  forgetMemory(id: string): Promise<void>;
  forgetAllMemory(): Promise<number>;
  /** La boîte de commande : décrire ce qu'il faut changer, plutôt que le faire à la main. */
  runMemoryCommand(instruction: string): Promise<MemoryCommandResult>;
  listExtensions(): Promise<InstalledExtension[]>;
  /** La liste canonique des capacités, telle que le serveur la connaît. */
  listCapabilities(): Promise<Capability[]>;
  // --- Noyaux alternatifs (extensions avec une section `core`) -------------
  coreStatus(id: string): Promise<CoreStatus>;
  coreStart(id: string): Promise<CoreStatus>;
  coreStop(id: string): Promise<CoreStatus>;
  /** Skills de l'écosystème du noyau, depuis l'index déclaré. */
  coreSkills(id: string): Promise<CoreSkillEntry[]>;
  /** Installe un skill via la commande déclarée par l'extension. */
  coreInstallSkill(id: string, slug: string): Promise<string>;
  /**
   * `source` is `owner/repo`, a git URL, a `github:owner/repo@ref#subdir`
   * spec, or a local directory. Installs disabled: permissions come next.
   */
  installExtension(source: string, scope?: string): Promise<InstalledExtension>;
  setExtensionEnabled(id: string, enabled: boolean): Promise<InstalledExtension[]>;
  /** Replaces the granted set — anything omitted is revoked. */
  setExtensionPermissions(
    id: string,
    granted: ExtensionPermission[],
  ): Promise<InstalledExtension[]>;
  /** Réinstalle depuis la source enregistrée, en conservant permissions et
   *  état actif/désactivé. */
  updateExtension(id: string): Promise<InstalledExtension>;
  /** Même chose avec une source explicite (entrée du catalogue « Découvrir »),
   *  pour une extension installée sans source enregistrée. */
  updateExtensionSource(id: string, source: string): Promise<InstalledExtension>;
  /** Compare chaque extension installée à sa source GitHub (badge mise à jour). */
  checkExtensionUpdates(): Promise<ExtensionUpdateCheck[]>;
  /** Un seul reload du runtime — convergence après la mise à jour en lot. */
  reloadExtensions(): Promise<InstalledExtension[]>;
  /** Aperçu du manifeste d'une source sans l'installer (carte de confirmation). */
  previewExtensionSource(source: string): Promise<ExtensionSourcePreview>;
  removeExtension(id: string): Promise<InstalledExtension[]>;
  /** Le formulaire déclaré par l'extension, et ses valeurs actuelles. */
  getExtensionConfig(id: string): Promise<ExtensionConfig>;
  /** Fusionne un patch dans les réglages. Les clés hors schéma sont ignorées. */
  setExtensionConfig(id: string, values: Record<string, unknown>): Promise<ExtensionConfig>;
  /** Les serveurs MCP déclarés par l'extension (env + auto-démarrage). */
  getExtensionMcpServers(id: string): Promise<ExtensionMcpServer[]>;
  /** Réécrit env + auto_start des serveurs déclarés, puis les redémarre. */
  setExtensionMcpServers(id: string, servers: ExtensionMcpServer[]): Promise<ExtensionMcpServer[]>;
  listExtensionCommands(): Promise<ExtensionCommand[]>;
  resolveExtensionCommand(name: string, args: string): Promise<string>;
  /** Lit le contenu textuel d'un asset d'extension (script, html, style). */
  readExtensionAsset(extensionId: string, assetPath: string): Promise<string>;
  /** Comme `readExtensionAsset`, mais suit l'adresse `refreshUrl` que l'asset
   *  déclare : un catalogue livré dans un paquet ne vieillit plus. Repli sur
   *  la dernière copie valide, puis sur le fichier du paquet. */
  refreshExtensionAsset(extensionId: string, assetPath: string): Promise<string>;
  /** Reads the last refresh. Filtering happens in Rust. */
  browseExtensionCatalog(opts?: {
    query?: string;
    ecosystem?: ExtensionEcosystem | null;
    limit?: number;
  }): Promise<CatalogSnapshot>;
  /** Hits the network. Slow — call it on demand, not on mount. */
  refreshExtensionCatalog(): Promise<CatalogSnapshot>;
  listCatalogSources(): Promise<CatalogSource[]>;
  addCatalogSource(spec: string): Promise<CatalogSource[]>;
  setCatalogSourceEnabled(id: string, enabled: boolean): Promise<CatalogSource[]>;
  removeCatalogSource(id: string): Promise<CatalogSource[]>;

  /** Travel mode — the tunnel lives in the daemon, so it outlives the window. */
  travelStatus(): Promise<TravelStatus>;
  travelRelays(): Promise<RelayChoice[]>;
  /** `null` switches it off. */
  setTravelMode(provider: string | null): Promise<TravelStatus>;
  /** The code that puts a phone back on the local network. */
  travelHomeCode(): Promise<TravelStatus>;
  /** Le code d'appairage d'un téléphone : `local`, `public` ou `tunnel`. */
  pairingCode(mode: PairingMode, url?: string): Promise<PairingCode>;
  /** Le modèle qui nomme les conversations. `null` : aucun, rien ne tourne. */
  systemPrompt(): Promise<SystemPrompt>;
  /** `null` ou un texte vide : ne rien poser devant le modèle. */
  setSystemPrompt(texte: string | null): Promise<SystemPrompt>;
  /** Liste des modèles dont le débridage est actif. */
  listDebridedModels(): Promise<string[]>;
  /** Activer ou désactiver le débridage pour un modèle. */
  toggleModelDebridage(tag: string, active: boolean): Promise<string[]>;
  microModel(): Promise<MicroModel>;
  setMicroModel(model: string | null): Promise<MicroModel>;

  /** MCP servers — shared with the daemon through `mcp.json`. */
  listMcpServers(): Promise<McpServerInfo[]>;
  addMcpServer(args: AddMcpServerArgs): Promise<McpServerInfo[]>;
  removeMcpServer(name: string): Promise<McpServerInfo[]>;
  /** Start a server and return the tools it announced. */
  startMcpServer(name: string): Promise<string[]>;
  stopMcpServer(name: string): Promise<void>;
  /** Invoke a tool through the same MCP client used by the agent runtime. */
  invokeMcpTool(name: string, tool: string, args: Record<string, unknown>): Promise<unknown>;
  /** Invoke a tool exposed by any enabled extension through the generic bridge. */
  invokeExtensionTool(tool: string, args: Record<string, unknown>): Promise<string>;

  /** Android SDK, AVDs et émulateurs présents sur cette machine. Lecture seule. */
  diagnoseAndroidVm(): Promise<AndroidVmStatus>;
  /** Installe les composants manquants et crée l'AVD demandé. */
  setupAndroidVm(args: AndroidVmSetupArgs): Promise<AndroidVmStatus>;
  /** Démarre un AVD existant. */
  startAndroidVm(args: {
    avdName: string;
    memoryMb?: number;
    camera?: string;
    microphone?: string;
  }): Promise<AndroidVmStatus>;
  stopAndroidVm(args?: { consolePort?: number }): Promise<AndroidVmStatus>;
  /** Capture screen pixels plus semantic UI tree; OCR is optional and never required. */
  androidScreenProbe(args?: AndroidScreenArgs): Promise<AndroidScreenProbe>;
  /** Send a bounded, explicit screen action, then return a fresh probe. */
  androidScreenAction(args: AndroidScreenActionArgs): Promise<AndroidScreenProbe>;
  /** Save a browser-recorded audio blob for tools that require a local path. */
  writeTestAudio(audioBase64: string, mimeType: string): Promise<string>;
  removeTestAudio(path: string): Promise<void>;
  /** Copy a generated voice note to a user-selected destination. */
  saveAudioAs(sourcePath: string, destinationPath: string): Promise<void>;
  /** Legacy image artifact action; generation itself belongs to a plugin. */
  saveImageAs(sourcePath: string, destinationPath: string): Promise<void>;

  /** Send the user verdict+scope back to the runtime (doc 11 s6.5). */
  approveToolCall: (decision: ToolApprovalDecision) => Promise<void>;
  updateProviderModelParams(params: ModelParams): Promise<void>;
  getProviderModelParams(): Promise<ModelParams>;
  /** Inspect a HuggingFace repository before downloading one variant. */
  inspectHuggingFaceRepo(source: string, hfToken?: string): Promise<HfRepoInspection>;
  /** Install a model. `selection` prevents a multi-variant HF repository from
   * downloading every quantisation and checkpoint. `downloads` is a validated
   * companion-file plan supplied by an enabled extension catalogue. */
  pullModel(
    endpoint: string,
    model: string,
    onProgress?: (pct: number, status?: string) => void,
    heretic?: boolean,
    consent?: boolean,
    selection?: HfModelSelection,
    downloads?: import("./modelRegistry").ModelDownloadSource[],
  ): Promise<void>;
  /** Cancel one download (by model URL/name) or all when omitted. */
  cancelPullModel(model?: string): Promise<void>;
  deleteModel(endpoint: string, model: string): Promise<void>;
  searchOllamaLibrary(
    query: string,
    category?: string,
  ): Promise<import("./modelRegistry").OllamaLibraryModel[]>;
  /** Unified snapshot of what the local runtime can do (honest, install-based). */
  runtimeCapabilities(): Promise<RuntimeCapabilities>;
  /** LoRA adapters currently loaded on the running server (with live scales). */
  listLoraAdapters(): Promise<LoraAdapter[]>;
  /** Hot-swap LoRA adapter scales on the running server (0 = off). */
  setLoraAdapters(scales: LoraScale[]): Promise<void>;
  /** Index a block of text under a source name for a project's RAG store. */
  ragIndexText(projectId: string, source: string, text: string): Promise<RagStatus>;
  /** Current RAG index status for a project (chunk count, sources, dim). */
  ragStatus(projectId: string): Promise<RagStatus>;
  /** Delete a project's whole RAG index. */
  ragClear(projectId: string): Promise<void>;
  /** Preview retrieval: top-k chunks for a query (also used to test the index). */
  ragSearch(projectId: string, query: string, k?: number): Promise<RagHit[]>;
  /** Les moteurs d'inférence connus — intégrés et apportés par extension. */
  listInferenceEngines(): Promise<InferenceEngineInfo[]>;
  /** Rend un moteur actif et démarre son processus. */
  startInferenceEngine(engine: string, model?: string): Promise<InferenceEngineInfo>;
  /** Arrête le processus d'un moteur lancé par l'application. */
  stopInferenceEngine(engine: string): Promise<void>;
  /** La fin du journal d'un moteur d'extension — pourquoi il n'a pas démarré. */
  inferenceEngineLog(engine: string, lines?: number): Promise<string>;
  /** Status of the managed llama.cpp runtime (installed / up to date). */
  llamaRuntimeStatus(): Promise<LlamaRuntimeStatus>;
  /** Download + install the pinned llama.cpp runtime, streaming progress. */
  setupLlamaRuntime(
    variant?: "vulkan" | "cpu",
    onProgress?: (pct: number, status?: string) => void,
  ): Promise<LlamaRuntimeStatus>;
  checkHardware(): Promise<HardwareSpec>;

  /** Ce qui est actuellement en mémoire, et si le minuteur peut y toucher. */
  modelResidency(): Promise<ResidencyStatus>;
  /** Ce que donnerait le chargement de ce modèle, sans rien charger. */
  checkModelFit(model: string): Promise<ModelFit>;
  /** Les catalogues distants qu'apportent les extensions actives. */
  cloudProviders(): Promise<CloudProvider[]>;
  /** Enregistrer une clé dans le trousseau du système. Elle n'en ressort pas. */
  cloudProviderSetKey(provider: string, key: string): Promise<void>;
  /** Oublier la clé d'un fournisseur. */
  cloudProviderClearKey(provider: string): Promise<void>;
  /** La liste des modèles du fournisseur, relue chez lui quand elle a vieilli. */
  cloudProviderModels(provider: string, refresh?: boolean): Promise<CloudModel[]>;
  /** Choisir un modèle distant : il devient le modèle actif de la conversation. */
  cloudProviderSelect(provider: string, model: string): Promise<void>;
  /** La passerelle locale répond-elle ? */
  cloudProviderStatus(provider: string): Promise<CloudProviderStatus>;
  /** Démarrer la passerelle avec la commande déclarée par son manifeste. */
  cloudProviderStart(provider: string): Promise<CloudProviderStatus>;
  /** L'installer avec la commande déclarée par son manifeste. */
  cloudProviderInstall(provider: string): Promise<string>;
  /** Ouvrir son tableau de bord dans le navigateur du système. */
  cloudProviderOpenDashboard(provider: string): Promise<string>;

  /** Ce que la machine a, mesuré : mémoire libre et bandes passantes. */
  llmfitHardware(): Promise<LlmfitHardware>;
  /** Estimer d'un coup des fiches pas encore téléchargées. Les réponses
   *  reviennent dans l'ordre reçu. */
  llmfitCatalog(entries: LlmfitCatalogEntry[]): Promise<ModelFit[]>;
  /** Charge un modèle et l'épingle. Rejette avec le message du garde-fou
   *  quand la mémoire manque, sauf si `force` est demandé explicitement. */
  loadChatModel(model: string, force?: boolean): Promise<ResidencyStatus>;
  /** Décharge le modèle et rend la mémoire. */
  ejectChatModel(): Promise<ResidencyStatus>;
  cautionLevel(): Promise<CautionLevel>;
  setCautionLevel(level: CautionLevel): Promise<void>;
  getInferenceConfig(): Promise<InferenceConfig>;
  setInferenceConfig(config: InferenceConfig, consent?: boolean): Promise<void>;
  getProfilePreset(profile: InferenceProfile): Promise<InferenceConfig>;
  openModelsFolder(path?: string): Promise<void>;
  /** URL de deep link (`locaryn://…`) qui a ouvert l'app, si elle en a reçu une.
   *  Lue une fois au démarrage ; les liens suivants arrivent par événement. */
  pendingDeepLink(): Promise<string | null>;
}

// ============================================================================
// Real implementation — Tauri IPC to the embedded Rust core
// ============================================================================

const HF_TOKEN_KEY = "locaryn_hf_token_v1";

/** HuggingFace access token, for gated repos (e.g. kyutai/pocket-tts). */
export function getHfToken(): string {
  try {
    return (typeof localStorage !== "undefined" && localStorage.getItem(HF_TOKEN_KEY)) || "";
  } catch {
    return "";
  }
}

/** Persist (or clear, when empty) the HuggingFace access token. */
export function setHfToken(token: string): void {
  try {
    const t = token.trim();
    if (t) localStorage.setItem(HF_TOKEN_KEY, t);
    else localStorage.removeItem(HF_TOKEN_KEY);
  } catch {
    // localStorage unavailable — the token just won't persist.
  }
}

const tauriCore: CoreApi = {
  health: () => invoke<Health>("core_health"),
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  listProjects: () => invoke<Project[]>("list_projects"),
  createProject: (path, name, trustLevel) =>
    invoke<Project>("create_project", { path, name, trustLevel }),
  updateProject: (id, name, trustLevel) =>
    invoke<Project>("update_project", { id, name: name ?? null, trustLevel: trustLevel ?? null }),
  archiveProject: (id) => invoke<void>("archive_project", { id }),
  freeChatProject: () => invoke<Project>("free_chat_project"),
  sessionWorkspace: (sessionId) => invoke<string>("session_workspace", { sessionId }),
  suggestFollowups: (answer, question) =>
    invoke<string[]>("suggest_followups", { answer, question }),
  planTask: (request) => invoke<TaskPlan>("plan_task", { request }),
  appendChatMessage: (sessionId, role, content) =>
    invoke<void>("append_chat_message", { sessionId, role, content }),
  appendAssistantMessage: (sessionId, content) =>
    invoke<void>("append_assistant_message", { sessionId, content }),
  planModelRuntime: (model) => invoke<RuntimePlan>("plan_model_runtime", { model }),
  getImageDefaults: () => invoke<ImageDefaults>("get_image_defaults"),
  setImageDefaults: (config) => invoke<void>("set_image_defaults", { config }),
  getModelPreferences: () => invoke<ModelPreferences>("get_model_preferences"),
  setModelPreferences: (preferences) => invoke<void>("set_model_preferences", { preferences }),
  listSessions: (projectId) => invoke<Session[]>("list_sessions", { projectId }),
  createSession: (projectId, title, coreId) =>
    invoke<Session>("create_session", { projectId, title: title ?? null, coreId: coreId ?? null }),
  updateSessionTitle: (sessionId, title) =>
    invoke<void>("update_session_title", { sessionId, title }),
  generateSessionTitle: (sessionId, firstPrompt) =>
    invoke<string>("generate_session_title", { sessionId, firstPrompt }),
  deleteSession: (sessionId) => invoke<void>("delete_session", { id: sessionId }),
  archiveSession: (sessionId, archived) =>
    invoke<void>("archive_session", { id: sessionId, archived }),
  archivedSessions: (projectId) => invoke<Session[]>("archived_sessions", { projectId }),
  runComposerTool: (tool, text) => invoke<string>("run_composer_tool", { tool, text }),
  moveSession: (sessionId, projectId) => invoke<void>("move_session", { id: sessionId, projectId }),
  suggestProject: (sessionId) => invoke("suggest_project", { sessionId }),
  mergeSessions: (sessionId, sourceId) => invoke<void>("merge_sessions", { sessionId, sourceId }),
  renameSession: (sessionId, title) => invoke<void>("rename_session", { id: sessionId, title }),
  createEphemeralSession: (projectId) => invoke<Session>("create_ephemeral_session", { projectId }),
  listFigures: () => invoke<Figure[]>("list_figures"),
  saveFigure: (f) =>
    invoke<Figure>("save_figure", {
      name: f.name,
      description: f.description,
      instructions: f.instructions,
      model: f.model,
      opening: f.opening,
      usesMemory: f.usesMemory,
      tools: f.tools
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean),
    }),
  deleteFigure: (id) => invoke<void>("delete_figure", { id }),
  attachFigure: (sessionId, figureId) => invoke<void>("attach_figure", { sessionId, figureId }),
  figureSessions: (figureId) => invoke<Session[]>("figure_sessions", { figureId }),
  listMessages: (sessionId) => invoke<Message[]>("list_messages", { sessionId }),

  sendMessage(sessionId, content, onEvent, images, responseFormat, reasoning) {
    const chan = new Channel<StreamEvent>();
    chan.onmessage = onEvent;
    return invoke("send_message", {
      sessionId,
      content,
      images: images ?? null,
      responseFormat: responseFormat ?? null,
      reasoning: reasoning ?? null,
      onEvent: chan,
    });
  },

  runTerminal(command, cwd, onOutput) {
    const chan = new Channel<TerminalEvent>();
    chan.onmessage = onOutput;
    return invoke("run_terminal", { command, cwd, onOutput: chan });
  },

  listProviders: () => invoke<Provider[]>("list_providers"),
  setActiveProvider: (id) => invoke<Provider>("set_active_provider", { id }),
  configureProvider: (endpoint, model) =>
    invoke<Provider>("configure_provider", { endpoint, model }),

  // ── AirLLM (low-VRAM inference engine) ────────────────────────────────
  airllmStatus: () =>
    invoke<{
      python: boolean;
      pythonPath?: string | null;
      torch: boolean;
      airllmInstalled: boolean;
      installed: { repo: string; installedAt: string }[];
    }>("airllm_status"),
  airllmSetup: (onLine) => {
    const chan = new Channel<
      { type: "line"; text: string } | { type: "done" } | { type: "error"; text: string }
    >();
    if (onLine)
      chan.onmessage = (m) => {
        if (m.type === "line") onLine(m.text);
      };
    return invoke<void>("airllm_setup", { onEvent: chan });
  },
  airllmInstall: (repo, onLine) => {
    const chan = new Channel<
      { type: "line"; text: string } | { type: "done" } | { type: "error"; text: string }
    >();
    if (onLine)
      chan.onmessage = (m) => {
        if (m.type === "line") onLine(m.text);
      };
    return invoke<{ repo: string; installedAt: string }>("airllm_install", {
      repo,
      onEvent: chan,
    });
  },
  airllmInstalled: () => invoke<{ repo: string; installedAt: string }[]>("airllm_installed"),
  airllmUninstall: (repo) => invoke<void>("airllm_uninstall", { repo }),
  configureAirllmProvider: (repo) => invoke<Provider>("configure_airllm_provider", { repo }),
  listModels: (endpoint) => invoke<string[]>("list_models", { endpoint }),
  listIncompatibleModels: () => invoke<string[]>("list_incompatible_models"),
  listNonChatModels: () => invoke<StoredWeight[]>("list_non_chat_models"),
  appInfo: () => invoke<AppInfo>("app_info"),
  getLocalProfile: () => invoke<LocalProfile>("get_local_profile"),
  setLocalProfile: (displayName) => invoke<LocalProfile>("set_local_profile", { displayName }),
  setLocalAvatar: (sourcePath) => invoke<LocalProfile>("set_local_avatar", { sourcePath }),
  clearLocalAvatar: () => invoke<LocalProfile>("clear_local_avatar"),

  listVoicePresets: () => invoke<VoicePreset[]>("list_voice_presets"),
  saveVoicePreset: (args) => invoke<VoicePreset>("save_voice_preset", { args }),
  deleteVoicePreset: (id) => invoke("delete_voice_preset", { id }),
  voicePresetSupport: (model) => invoke<EngineSupport>("voice_preset_support", { model }),

  serverStatus: () => invoke<ServerStatus>("server_status"),
  setServerMode: (enabled, port) =>
    invoke<ServerStatus>("set_server_mode", { args: { enabled, port: port ?? null } }),
  restartServer: () => invoke<ServerStatus>("restart_server"),
  listServerUsers: () => invoke<ServerUserSummary[]>("list_server_users"),
  createServerUser: (username, password, isAdmin = true) =>
    invoke<ServerStatus>("create_server_user", {
      args: { username, password, isAdmin },
    }),
  deleteServerUser: (userId) => invoke<ServerStatus>("delete_server_user", { userId }),
  provisioning: () => invoke<Provisioning | null>("provisioning"),

  signIn: (serverUrl, username, password) =>
    invoke<ServerSession>("sign_in", { serverUrl, username, password }),
  currentSession: () => invoke<ServerSession | null>("current_session"),
  signOut: () => invoke<void>("sign_out"),

  clientCertificateStatus: () => invoke<CertificateStatus>("client_certificate_status"),
  installClientCertificate: (source, authority) =>
    invoke<CertificateStatus>("install_client_certificate", {
      source,
      authority: authority ?? null,
    }),
  removeClientCertificate: () => invoke<CertificateStatus>("remove_client_certificate"),

  storageInfo: () => invoke<StorageInfo>("storage_info"),
  setStorageRoot: (newRoot, moveData) =>
    invoke<StorageInfo>("set_storage_root", { args: { new_root: newRoot, move_data: moveData } }),
  cleanTemp: () => invoke<number>("clean_temp"),

  listConnectorTypes: () => invoke<ConnectorType[]>("list_connector_types"),

  listModelMetrics: () => invoke<ModelMetric[]>("list_model_metrics"),
  listMemory: () => invoke<MemoryEntry[]>("list_memory"),
  remember: (group, title, detail) => invoke<MemoryEntry>("remember", { group, title, detail }),
  setMemorySummary: (id, summary) => invoke<MemoryEntry>("set_memory_summary", { id, summary }),
  renameMemoryEntry: (id, title) => invoke<MemoryEntry>("rename_memory_entry", { id, title }),
  setMemoryGroup: (id, group) => invoke<MemoryEntry>("set_memory_group", { id, group }),
  removeMemoryDetail: (id, detail) => invoke<MemoryEntry>("remove_memory_detail", { id, detail }),
  forgetMemory: (id) => invoke<void>("forget_memory", { id }),
  forgetAllMemory: () => invoke<number>("forget_all_memory"),
  runMemoryCommand: (instruction) =>
    invoke<MemoryCommandResult>("run_memory_command", { instruction }),
  listExtensions: () => invoke<InstalledExtension[]>("list_extensions"),
  listCapabilities: () => invoke<Capability[]>("list_capabilities"),
  coreStatus: (id) => invoke<CoreStatus>("core_status", { id }),
  coreStart: (id) => invoke<CoreStatus>("core_start", { id }),
  coreStop: (id) => invoke<CoreStatus>("core_stop", { id }),
  coreSkills: (id) => invoke<CoreSkillEntry[]>("core_skills", { id }),
  coreInstallSkill: (id, slug) => invoke<string>("core_install_skill", { id, slug }),
  installExtension: (source, scope) =>
    invoke<InstalledExtension>("install_extension", { source, scope }),
  setExtensionEnabled: (id, enabled) =>
    invoke<InstalledExtension[]>("set_extension_enabled", { id, enabled }),
  setExtensionPermissions: (id, granted) =>
    invoke<InstalledExtension[]>("set_extension_permissions", { id, granted }),
  updateExtension: (id) => invoke<InstalledExtension>("update_extension", { id }),
  updateExtensionSource: (id, source) =>
    invoke<InstalledExtension>("update_extension_source", { id, source }),
  checkExtensionUpdates: () => invoke<ExtensionUpdateCheck[]>("check_extension_updates"),
  reloadExtensions: () => invoke<InstalledExtension[]>("reload_extensions"),
  previewExtensionSource: (source) =>
    invoke<ExtensionSourcePreview>("preview_extension_source", { source }),
  removeExtension: (id) => invoke<InstalledExtension[]>("remove_extension", { id }),
  getExtensionConfig: (id) => invoke<ExtensionConfig>("get_extension_config", { id }),
  setExtensionConfig: (id, values) =>
    invoke<ExtensionConfig>("set_extension_config", { id, values }),
  getExtensionMcpServers: (id) => invoke<ExtensionMcpServer[]>("get_extension_mcp_servers", { id }),
  setExtensionMcpServers: (id, servers) =>
    invoke<ExtensionMcpServer[]>("set_extension_mcp_servers", { id, servers }),
  listExtensionCommands: () => invoke<ExtensionCommand[]>("list_extension_commands"),
  resolveExtensionCommand: (name, args) =>
    invoke<string>("resolve_extension_command", { name, args }),
  readExtensionAsset: (extensionId, assetPath) =>
    invoke<string>("read_extension_asset", { extensionId, assetPath }),
  refreshExtensionAsset: (extensionId, assetPath) =>
    invoke<string>("refresh_extension_asset", { extensionId, assetPath }),
  browseExtensionCatalog: (opts) =>
    invoke<CatalogSnapshot>("browse_extension_catalog", {
      query: opts?.query ?? null,
      ecosystem: opts?.ecosystem ?? null,
      limit: opts?.limit ?? null,
    }),
  refreshExtensionCatalog: () => invoke<CatalogSnapshot>("refresh_extension_catalog"),
  listCatalogSources: () => invoke<CatalogSource[]>("list_catalog_sources"),
  addCatalogSource: (spec) => invoke<CatalogSource[]>("add_catalog_source", { spec }),
  setCatalogSourceEnabled: (id, enabled) =>
    invoke<CatalogSource[]>("set_catalog_source_enabled", { id, enabled }),
  removeCatalogSource: (id) => invoke<CatalogSource[]>("remove_catalog_source", { id }),

  travelStatus: () => invoke<TravelStatus>("travel_status"),
  travelRelays: () => invoke<RelayChoice[]>("travel_relays"),
  setTravelMode: (provider) => invoke<TravelStatus>("set_travel_mode", { args: { provider } }),
  travelHomeCode: () => invoke<TravelStatus>("travel_home_code"),
  pairingCode: (mode, url) => invoke<PairingCode>("pairing_code", { mode, url }),
  systemPrompt: () => invoke<SystemPrompt>("consigne_systeme"),
  setSystemPrompt: (texte) => invoke<SystemPrompt>("definir_consigne_systeme", { texte }),
  listDebridedModels: () => invoke<string[]>("modeles_debrides"),
  toggleModelDebridage: (tag: string, active: boolean) =>
    invoke<string[]>("basculer_debridage_modele", { tag, actif: active }),
  microModel: () => invoke<MicroModel>("micro_model"),
  setMicroModel: (model) => invoke<MicroModel>("set_micro_model", { model }),

  listMcpServers: () => invoke<McpServerInfo[]>("list_mcp_servers"),
  addMcpServer: (args) =>
    invoke<McpServerInfo[]>("add_mcp_server", {
      args: {
        name: args.name,
        transport: args.transport,
        target: args.target,
        env: args.env ?? {},
        autoStart: args.autoStart ?? false,
      },
    }),
  removeMcpServer: (name) => invoke<McpServerInfo[]>("remove_mcp_server", { name }),
  startMcpServer: (name) => invoke<string[]>("start_mcp_server", { name }),
  stopMcpServer: (name) => invoke<void>("stop_mcp_server", { name }),
  invokeMcpTool: (name, tool, args) => invoke<unknown>("invoke_mcp_tool", { name, tool, args }),
  invokeExtensionTool: (tool, args) => invoke<string>("invoke_extension_tool", { tool, args }),
  diagnoseAndroidVm: () => invoke<AndroidVmStatus>("diagnose_android_vm"),
  setupAndroidVm: (args: AndroidVmSetupArgs) =>
    invoke<AndroidVmStatus>("setup_android_vm", { args }),
  startAndroidVm: (args: {
    avdName: string;
    memoryMb?: number;
    camera?: string;
    microphone?: string;
  }) => invoke<AndroidVmStatus>("start_android_vm", { args }),
  stopAndroidVm: (args = {}) => invoke<AndroidVmStatus>("stop_android_vm", { args }),
  androidScreenProbe: (args = {}) => invoke<AndroidScreenProbe>("android_screen_probe", { args }),
  androidScreenAction: (args) => invoke<AndroidScreenProbe>("android_screen_action", { args }),
  writeTestAudio: (audioBase64, mimeType) =>
    invoke<string>("write_test_audio", { audioBase64, mimeType }),
  removeTestAudio: (path) => invoke<void>("remove_test_audio", { path }),
  saveAudioAs: (sourcePath, destinationPath) =>
    invoke<void>("save_audio_as", { sourcePath, destinationPath }),
  saveImageAs: (sourcePath, destinationPath) =>
    invoke<void>("save_image_as", { sourcePath, destinationPath }),
  approveToolCall: (decision) => invoke("approve_tool_call", { payload: decision }),

  updateProviderModelParams: (params) => invoke("update_provider_model_params", { params }),
  getProviderModelParams: () => invoke<ModelParams>("get_provider_model_params"),
  inspectHuggingFaceRepo: (source, hfToken) =>
    invoke<HfRepoInspection>("inspect_huggingface_repo", {
      source,
      hfToken: hfToken || null,
    }),
  pullModel: (endpoint, model, onProgress, heretic, consent, selection, downloads) => {
    const chan = new Channel<{
      status: string;
      completed: number;
      total: number;
      percentage: number;
    }>();
    if (onProgress) {
      chan.onmessage = (ev) => {
        onProgress(Math.round(ev.percentage), ev.status);
      };
    }
    // Gated HuggingFace repos (kyutai/pocket-tts, Qwen3-TTS, …) answer 401
    // without an access token; the Rust side sends it only to huggingface.co.
    const hfToken = getHfToken();
    return invoke("pull_model", {
      endpoint,
      model,
      heretic: heretic ?? null,
      consent: consent ?? null,
      hfToken: hfToken || null,
      selection: selection ?? null,
      companions: downloads ?? null,
      onEvent: chan,
    });
  },
  cancelPullModel: (model) => invoke("cancel_pull_model", { model: model ?? null }),
  deleteModel: (endpoint, model) => invoke("delete_model_cmd", { endpoint, model }),
  searchOllamaLibrary: (query, category) =>
    invoke<import("./modelRegistry").OllamaLibraryModel[]>("search_ollama_library", {
      query,
      category,
    }),
  runtimeCapabilities: () => invoke<RuntimeCapabilities>("runtime_capabilities"),
  listLoraAdapters: () => invoke<LoraAdapter[]>("list_lora_adapters"),
  setLoraAdapters: (scales) => invoke<void>("set_lora_adapters", { scales }),
  ragIndexText: (projectId, source, text) =>
    invoke<RagStatus>("rag_index_text", { projectId, source, text }),
  ragStatus: (projectId) => invoke<RagStatus>("rag_status", { projectId }),
  ragClear: (projectId) => invoke<void>("rag_clear", { projectId }),
  ragSearch: (projectId, query, k) =>
    invoke<RagHit[]>("rag_search", { projectId, query, k: k ?? null }),
  listInferenceEngines: () => invoke<InferenceEngineInfo[]>("list_inference_engines"),
  startInferenceEngine: (engine, model) =>
    invoke<InferenceEngineInfo>("start_inference_engine", { engine, model: model ?? null }),
  stopInferenceEngine: (engine) => invoke<void>("stop_inference_engine", { engine }),
  inferenceEngineLog: (engine, lines) =>
    invoke<string>("inference_engine_log", { engine, lines: lines ?? null }),
  llamaRuntimeStatus: () => invoke<LlamaRuntimeStatus>("llama_runtime_status"),
  setupLlamaRuntime: (variant, onProgress) => {
    const chan = new Channel<{
      status: string;
      completed: number;
      total: number;
      percentage: number;
    }>();
    if (onProgress) {
      chan.onmessage = (ev) => onProgress(Math.round(ev.percentage), ev.status);
    }
    return invoke<LlamaRuntimeStatus>("setup_llama_runtime", {
      variant: variant ?? null,
      onEvent: chan,
    });
  },
  checkHardware: () => invoke("check_hardware"),

  modelResidency: () => invoke<ResidencyStatus>("model_residency"),
  checkModelFit: (model) => invoke<ModelFit>("check_model_fit", { model }),
  cloudProviders: () => invoke<CloudProvider[]>("cloud_providers"),
  cloudProviderSetKey: (provider, key) => invoke<void>("cloud_provider_set_key", { provider, key }),
  cloudProviderClearKey: (provider) => invoke<void>("cloud_provider_clear_key", { provider }),
  cloudProviderModels: (provider, refresh) =>
    invoke<CloudModel[]>("cloud_provider_models", { provider, refresh }),
  cloudProviderSelect: (provider, model) =>
    invoke<void>("cloud_provider_select", { provider, model }),
  cloudProviderStatus: (provider) =>
    invoke<CloudProviderStatus>("cloud_provider_status", { provider }),
  cloudProviderStart: (provider) =>
    invoke<CloudProviderStatus>("cloud_provider_start", { provider }),
  cloudProviderInstall: (provider) => invoke<string>("cloud_provider_install", { provider }),
  cloudProviderOpenDashboard: (provider) =>
    invoke<string>("cloud_provider_open_dashboard", { provider }),
  llmfitHardware: () => invoke<LlmfitHardware>("llmfit_hardware"),
  llmfitCatalog: (entries) => invoke<ModelFit[]>("llmfit_catalog", { entries }),
  loadChatModel: (model, force) =>
    invoke<ResidencyStatus>("load_chat_model", { model, force: force ?? null }),
  ejectChatModel: () => invoke<ResidencyStatus>("eject_chat_model"),
  cautionLevel: () => invoke<CautionLevel>("caution_level"),
  setCautionLevel: (level) => invoke<void>("set_caution_level", { level }),

  getInferenceConfig: () => invoke<InferenceConfig>("get_inference_config"),
  setInferenceConfig: (config, consent) =>
    invoke("set_inference_config", { config, consent: consent ?? null }),
  getProfilePreset: (profile) => invoke<InferenceConfig>("get_profile_preset", { profile }),
  openModelsFolder: (path) => invoke("open_models_folder", { path: path ?? null }),
  // Le plugin deep-link expose `get_current` : la première URL qui a ouvert
  // l'app, ou null. Rendu en tableau de chaînes côté serde.
  pendingDeepLink: async () => {
    try {
      const urls = await invoke<string[] | null>("plugin:deep-link|get_current");
      return urls?.[0] ?? null;
    } catch {
      return null;
    }
  },
};

// ============================================================================
// Demo implementation — canned data for browser-based UI development
// ============================================================================

let demoImageDefaults: ImageDefaults = {
  quality: "standard",
  width: 512,
  height: 512,
  steps: 0,
  cfg_scale: 0,
  vram_mode: "auto",
  negative_prompt: "",
  variants: 1,
};
let demoModelPreferences: ModelPreferences = { tts_model: null, image_model: null };
let demoLocalProfile: LocalProfile = { display_name: "", avatar_path: null };

const now = Date.now();
const iso = (msAgo: number) => new Date(now - msAgo).toISOString();
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const demoProject: Project = {
  id: "demo-project",
  path: "D:/Documents/Syncho",
  name: "syncho",
  trust_level: "trusted",
  created_at: iso(86_400_000 * 6),
  updated_at: iso(3_600_000),
  deleted_at: null,
};

const demoProject2: Project = {
  ...demoProject,
  id: "demo-project-2",
  path: "D:/Dev/orbital",
  name: "orbital",
  trust_level: "untrusted",
  updated_at: iso(86_400_000 * 2),
};

const demoSessions: Session[] = [
  {
    id: "demo-session-1",
    project_id: "demo-project",
    title: "Wire the preview panel",
    provider_id: null,
    model: "qwen2.5:3b",
    created_at: iso(7_200_000),
    last_message_at: iso(300_000),
    closed_at: null,
  },
  {
    id: "demo-session-2",
    project_id: "demo-project",
    title: null,
    provider_id: null,
    model: null,
    created_at: iso(86_400_000 * 2),
    last_message_at: iso(86_400_000 * 2),
    closed_at: null,
  },
];

const demoMessages: Message[] = [
  {
    id: "m1",
    session_id: "demo-session-1",
    role: "user",
    content: "Read the file README.md and tell me what this project is.",
    tool_calls: null,
    tool_call_id: null,
    tokens_in: 0,
    tokens_out: 0,
    parent_id: null,
    created_at: iso(400_000),
  },
  {
    id: "m2",
    session_id: "demo-session-1",
    role: "assistant",
    content:
      "**Locaryn** is an open-core agentic coding platform:\n\n- Native desktop (Tauri v2 + React) and a CLI sharing one Rust core\n- Local daemon with SQLite persistence\n- Provider supervisor for local runtimes (`ollama`, `llama.cpp`, …)\n\n```rust\nlet agent = OllamaAgent::with_defaults(None, None);\nlet stream = agent.run(input).await?;\n```\n\nSee `docs/architecture/10-roadmap.md` for the current milestone.",
    tool_calls: null,
    tool_call_id: null,
    tokens_in: 2007,
    tokens_out: 103,
    parent_id: null,
    created_at: iso(300_000),
  },
];

const demoHealth: Health = {
  status: "ok",
  version: "0.1.0-demo",
  mode: "local",
  active_provider: {
    kind: "local",
    engine: "ollama",
    endpoint: "http://127.0.0.1:8080",
    model: "qwen2.5:3b",
  },
};

let demoProviders: Provider[] = [
  {
    id: "demo-ollama",
    kind: "local",
    engine: "ollama",
    endpoint: "http://127.0.0.1:8080",
    model: "qwen2.5:3b",
    is_active: true,
    status: "healthy",
    config: null,
    created_at: iso(86_400_000 * 6),
    updated_at: iso(3_600_000),
  },
];

// `llama3.1:70b` est là exprès : sans un modèle que la machine simulée ne peut
// pas tenir, le refus du garde-fou mémoire serait inatteignable depuis
// l'interface, et donc invérifiable ailleurs qu'en production.
const demoModels = [
  "qwen2.5:3b",
  "qwen2.5-coder:7b",
  "llama3.2:3b",
  "mistral:7b",
  "phi3:mini",
  "llama3.1:70b",
];

const demoConnectorTypes: ConnectorType[] = [
  {
    type_id: "mcp_custom",
    display_name: "Serveur MCP Personnalisé",
    summary:
      "Ajoutez n'importe quel serveur Model Context Protocol (STDIO ou SSE) via sa commande ou son URL HTTP.",
    icon: "extensions",
    category: "extension",
    source: "built-in",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "github",
    display_name: "GitHub MCP Server",
    summary:
      "Recherche de dépôts, gestion des issues, Pull Requests et suivi des workflows GitHub Actions.",
    icon: "cloud",
    category: "extension",
    source: "built-in",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "gitlab",
    display_name: "GitLab MCP Integration",
    summary:
      "Intégration Merge Requests, pipelines CI/CD et parcours de projets sur votre instance GitLab.",
    icon: "cloud",
    category: "extension",
    source: "built-in",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "web_search",
    display_name: "Brave Web Search & Scraper",
    summary: "Recherche web en direct et extraction automatique de documentation technique à jour.",
    icon: "search",
    category: "plugin",
    source: "built-in",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "postgres",
    display_name: "PostgreSQL & MySQL MCP",
    summary:
      "Inspection du schéma de base de données, requêtes SQL sécurisées en lecture seule et assistance à la migration.",
    icon: "models",
    category: "connector",
    source: "built-in",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "docker",
    display_name: "Docker & Kubernetes Engine",
    summary:
      "Gestion des conteneurs locaux/distants, inspection des logs et commandes kubectl assistées.",
    icon: "cube",
    category: "connector",
    source: "built-in",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "lsp",
    display_name: "Language Server Protocol (LSP)",
    summary:
      "Intégration Pyright, rust-analyzer, tsserver et gopls pour l'analyse de type et autocomplétion exacte.",
    icon: "speed",
    category: "plugin",
    source: "built-in",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "python_sandbox",
    display_name: "Python Code Interpreter",
    summary:
      "Exécution sécurisée de scripts Python, calculs scientifiques et génération de graphiques en bac à sable.",
    icon: "cpu",
    category: "plugin",
    source: "built-in",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "playwright",
    display_name: "Playwright Web Automator",
    summary:
      "Tests E2E, captures d'écran et automation de navigateurs web directement depuis l'agent.",
    icon: "figures",
    category: "plugin",
    source: "built-in",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "notion_jira",
    display_name: "Notion & Jira Sync",
    summary:
      "Lecture des spécifications de projet, tickets Jira, tâches et documentation d'équipe.",
    icon: "models",
    category: "extension",
    source: "community",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "memory_rag",
    display_name: "Graphe de Mémoire & RAG",
    summary:
      "Mémoire vectorielle persistante entre les sessions de chat pour conserver le contexte du projet.",
    icon: "memory",
    category: "extension",
    source: "built-in",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "cloud_deploy",
    display_name: "AWS / Vercel / Cloudflare",
    summary: "Déploiement en un clic de vos applications et suivi des logs de production.",
    icon: "cloud",
    category: "connector",
    source: "community",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  {
    type_id: "slack_discord",
    display_name: "Slack & Discord Connector",
    summary: "Notifications en direct et import d'extraits de discussions d'équipe dans l'agent.",
    icon: "chat",
    category: "connector",
    source: "community",
    available: true,
    supports_test: false,
    install_hint: "",
  },
  // Official MCP reference servers (modelcontextprotocol/servers) — mirrors the
  // Rust catalog so the demo and the real app show the same entries.
  ...(
    [
      [
        "mcp_filesystem",
        "Filesystem",
        "project",
        "Lecture/écriture de fichiers avec contrôle d'accès configurable.",
        "filesystem",
      ],
      ["mcp_git", "Git", "forward", "Lire, chercher et manipuler des dépôts Git.", "git"],
      [
        "mcp_fetch",
        "Fetch",
        "translate",
        "Récupère une page web et la convertit pour le modèle.",
        "fetch",
      ],
      [
        "mcp_memory",
        "Memory",
        "memory",
        "Mémoire persistante sous forme de graphe de connaissances.",
        "memory",
      ],
      [
        "mcp_sequential",
        "Sequential Thinking",
        "chat",
        "Résolution de problèmes par séquences de raisonnement.",
        "sequential-thinking",
      ],
      ["mcp_time", "Time", "clock", "Date, heure et conversions de fuseaux horaires.", "time"],
      [
        "mcp_everything",
        "Everything (démo)",
        "shield",
        "Serveur de référence exposant prompts, ressources et outils.",
        "everything",
      ],
    ] as const
  ).map(([id, name, icon, summary, pkg]) => ({
    type_id: id,
    display_name: name,
    summary,
    icon,
    category: "extension",
    source: "MCP officiel",
    available: true,
    supports_test: false,
    install_hint: `npx -y @modelcontextprotocol/server-${pkg}`,
  })),
  {
    type_id: "mcp_brave",
    display_name: "Brave Search",
    summary: "Recherche web et locale via l'API Brave (remplace le serveur de référence archivé).",
    icon: "cloud",
    category: "extension",
    source: "MCP communauté",
    available: true,
    supports_test: false,
    install_hint: "npx -y @brave/brave-search-mcp-server",
  },
];

let demoSessionCounter = 2;

// Return a fresh copy so React sees a new reference (the real Tauri core
// deserializes a new object on every call; the demo must match that).
const cloneHealth = (): Health => ({
  ...demoHealth,
  active_provider: demoHealth.active_provider ? { ...demoHealth.active_provider } : null,
});

// Extensions, in the browser demo. One entry per ecosystem so the store's
// grouping, the compatibility badges and the permission modal are all
// exercised without a backend.
// ── Fournisseur distant de démonstration ───────────────────────────────────
// Le parcours complet — dossier dans « Mes modèles », page du fournisseur,
// dossier dans le sélecteur du chat — doit être exerçable hors Tauri, sinon
// il ne se vérifie que sur une machine et à la main.

const demoCloudProviders: CloudProvider[] = [
  {
    id: "omniroute",
    label: "OmniRoute",
    extension_id: "demo-omniroute",
    extension_name: "morph-omniroute",
    api_url: "http://localhost:20128",
    models_url: "http://localhost:20128/v1/models",
    keys_url: "http://localhost:20128",
    docs_url: "https://github.com/pitbaden/omniroute#readme",
    key_hint: "Clé émise par OmniRoute (page « Endpoints »)",
    has_key: false,
    model_count: 0,
    updated_at: null,
    active_model: null,
    is_local: true,
    dashboard_url: "http://localhost:20128",
    install_hint: "npm install -g omniroute",
    can_start: true,
    can_install: true,
    installed: false,
  },
];

// Ce qu'OmniRoute expose une fois ses fournisseurs connectés : les modèles de
// tout le monde, derrière une seule adresse et une seule clé.
const demoCloudModels: CloudModel[] = [
  {
    id: "anthropic/claude-opus-5",
    name: "Claude Opus 5",
    description: "Routé vers Anthropic, avec repli automatique.",
    context_length: 1_000_000,
    prompt_price_per_m: 5,
    completion_price_per_m: 25,
    modality: "text+image->text",
    supports_tools: true,
  },
  {
    id: "openai/gpt-5",
    name: "GPT-5",
    description: "Routé vers OpenAI.",
    context_length: 400_000,
    prompt_price_per_m: 1.25,
    completion_price_per_m: 10,
    modality: "text+image->text",
    supports_tools: true,
  },
  {
    id: "google/gemini-3-pro",
    name: "Gemini 3 Pro",
    description: "Très longue fenêtre de contexte.",
    context_length: 2_000_000,
    prompt_price_per_m: 1.25,
    completion_price_per_m: 5,
    modality: "text+image->text",
    supports_tools: true,
  },
  {
    id: "meta-llama/llama-4-maverick:free",
    name: "Llama 4 Maverick (quota gratuit)",
    description: "Repli gratuit tant que le quota tient.",
    context_length: 128_000,
    prompt_price_per_m: 0,
    completion_price_per_m: 0,
    modality: "text->text",
    supports_tools: false,
  },
];

/** L'état de la passerelle en démonstration : allumée ou non, clé, modèle. */
const demoCloudState = {
  key: "",
  model: null as string | null,
  running: false,
  installed: false,
};

// Aucune extension installée : c'est l'état d'une application fraîche.
//
// Le mode démo montrait cinq morphs déjà en place, ce qui donnait un Studio,
// un écran d'entraînement et des capacités que personne n'avait installés.
// On ne teste plus l'interface contre un état que l'utilisateur n'a jamais.
let demoExtensions: InstalledExtension[] = [];

// Un schéma de démonstration, pour que le rendu du formulaire soit exerçable
// hors Tauri. Il vient d'une extension fictive : l'application ne connaît
// aucun de ces champs, elle ne fait que les dessiner.
const demoExtensionConfigs: Record<string, ExtensionConfig> = {
  "demo-2": {
    schema: {
      "persona.style": {
        type: "text",
        title: "Personnalité et ton",
        description: "Décrivez librement comment l'assistant doit se comporter.",
        default: "Ton direct et concret, sans jargon inutile.",
        group: "Personnalité",
      },
      "persona.language": {
        type: "select",
        title: "Langue des réponses",
        options: ["auto", "fr", "en"],
        optionLabels: ["Celle du message reçu", "Français", "Anglais"],
        default: "auto",
        group: "Personnalité",
      },
      "limits.enabled": {
        type: "boolean",
        title: "Activer les garde-fous",
        description: "Décoché, l'assistant répond à tout sans limite ni escalade.",
        default: true,
        group: "Garde-fous",
      },
      "limits.max_reply_chars": {
        type: "number",
        title: "Longueur maximale d'une réponse",
        default: 900,
        min: 200,
        max: 4000,
        group: "Garde-fous",
      },
      "limits.escalation_keywords": {
        type: "list",
        title: "Mots déclenchant une escalade",
        default: ["urgent", "facture"],
        group: "Garde-fous",
      },
      "transport.telegram_api_id": {
        type: "number",
        title: "Identifiant API (my.telegram.org)",
        default: 0,
        group: "Canal",
      },
      "transport.telegram_api_hash": {
        type: "secret",
        title: "Clé API (my.telegram.org)",
        default: "",
        group: "Canal",
      },
      "transport.telegram_session_file": {
        type: "string",
        title: "Fichier de session",
        default: ".telegram/session.txt",
        group: "Canal",
      },
      "voice.reference_sample": {
        type: "path",
        title: "Échantillon de voix à cloner",
        default: "",
        group: "Voix",
      },
    },
    values: {
      "persona.style": "Ton direct et concret, sans jargon inutile.",
      "persona.language": "fr",
      "limits.enabled": true,
      "limits.max_reply_chars": 900,
      "limits.escalation_keywords": ["urgent", "facture"],
      "transport.telegram_api_id": 0,
      "transport.telegram_api_hash": "",
      "transport.telegram_session_file": ".telegram/session.txt",
      "voice.reference_sample": "",
    },
  },
};

const demoCatalog: CatalogEntry[] = [
  {
    id: "locaryn:morph-image",
    name: "morph-image",
    display_name: "Image",
    description:
      "Génération et retouche d'images IA avec stable-diffusion.cpp et studio de création complet.",
    author: "Locaryn Team",
    version: "3.1.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-image",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-image#v3.1.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "3.1.0-beta.1",
        tag: "v3.1.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (3.1.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-image#v3.1.0-beta.1",
      },
      {
        version: "3.0.0",
        tag: "v3.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v3.0.0",
        install_source: "Locaryn/morph-image#v3.0.0",
      },
      {
        version: "2.2.0",
        tag: "v2.2.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.2.0",
        install_source: "Locaryn/morph-image#v2.2.0",
      },
      {
        version: "2.1.0",
        tag: "v2.1.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.1.0",
        install_source: "Locaryn/morph-image#v2.1.0",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-image#v2.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-voice-tts",
    name: "morph-voice-tts",
    display_name: "Synthèse Vocale (TTS)",
    description: "Synthèse vocale multilingue & clonage de voix haute fidélité (Kokoro, XTTS).",
    author: "Locaryn Team",
    version: "2.2.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-voice-tts",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-voice-tts#v2.2.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.2.0-beta.1",
        tag: "v2.2.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.2.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-voice-tts#v2.2.0-beta.1",
      },
      {
        version: "2.1.0",
        tag: "v2.1.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.1.0",
        install_source: "Locaryn/morph-voice-tts#v2.1.0",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-voice-tts#v2.0.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-voice-tts#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-dictaphone",
    name: "morph-dictaphone",
    display_name: "Dictaphone & STT",
    description:
      "Dictée vocale et transcription continue Speech-to-Text pour le compositeur de message.",
    author: "Locaryn Team",
    version: "2.2.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-dictaphone",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-dictaphone#v2.2.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.2.0-beta.1",
        tag: "v2.2.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.2.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-dictaphone#v2.2.0-beta.1",
      },
      {
        version: "2.1.0",
        tag: "v2.1.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.1.0",
        install_source: "Locaryn/morph-dictaphone#v2.1.0",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-dictaphone#v2.0.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-dictaphone#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-video-gen",
    name: "morph-video-gen",
    display_name: "Génération Vidéo",
    description: "Génération et animation de clips vidéo IA à partir de prompts ou d'images.",
    author: "Locaryn Team",
    version: "2.1.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-video-gen",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-video-gen#v2.1.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.1.0-beta.1",
        tag: "v2.1.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.1.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-video-gen#v2.1.0-beta.1",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-video-gen#v2.0.0",
      },
      {
        version: "1.5.0",
        tag: "v1.5.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.5.0",
        install_source: "Locaryn/morph-video-gen#v1.5.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-video-gen#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-3d-gen",
    name: "morph-3d-gen",
    display_name: "Génération 3D",
    description: "Création de maillages 3D, textures et modèles spatiaux (TripoSR, GLTF).",
    author: "Locaryn Team",
    version: "2.1.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-3d-gen",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-3d-gen#v2.1.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.1.0-beta.1",
        tag: "v2.1.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.1.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-3d-gen#v2.1.0-beta.1",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-3d-gen#v2.0.0",
      },
      {
        version: "1.5.0",
        tag: "v1.5.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.5.0",
        install_source: "Locaryn/morph-3d-gen#v1.5.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-3d-gen#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-music-gen",
    name: "morph-music-gen",
    display_name: "Composition Musicale",
    description: "Génération de pistes audio instrumentales et musiques IA avec MusicGen.",
    author: "Locaryn Team",
    version: "2.1.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-music-gen",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-music-gen#v2.1.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.1.0-beta.1",
        tag: "v2.1.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.1.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-music-gen#v2.1.0-beta.1",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-music-gen#v2.0.0",
      },
      {
        version: "1.5.0",
        tag: "v1.5.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.5.0",
        install_source: "Locaryn/morph-music-gen#v1.5.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-music-gen#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-vision-ocr",
    name: "morph-vision-ocr",
    display_name: "Vision & OCR",
    description: "Reconnaissance optique de texte et analyse multimodale d'images par ordinateur.",
    author: "Locaryn Team",
    version: "2.1.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-vision-ocr",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-vision-ocr#v2.1.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.1.0-beta.1",
        tag: "v2.1.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.1.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-vision-ocr#v2.1.0-beta.1",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-vision-ocr#v2.0.0",
      },
      {
        version: "1.5.0",
        tag: "v1.5.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.5.0",
        install_source: "Locaryn/morph-vision-ocr#v1.5.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-vision-ocr#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-figures",
    name: "morph-figures",
    display_name: "Figures",
    description: "Personnalités spécialisées, consignes métier expertes et agents dédiés.",
    author: "Locaryn Team",
    version: "1.1.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-figures",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-figures#v1.1.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "1.1.0-beta.1",
        tag: "v1.1.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (1.1.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-figures#v1.1.0-beta.1",
      },
      {
        version: "1.0.1",
        tag: "v1.0.1",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.1",
        install_source: "Locaryn/morph-figures#v1.0.1",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-figures#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-rag-qa",
    name: "morph-rag-qa",
    display_name: "RAG Documentaire",
    description: "Recherche documentaire vectorielle et questions-réponses sémantiques.",
    author: "Locaryn Team",
    version: "2.2.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-rag-qa",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-rag-qa#v2.2.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.2.0-beta.1",
        tag: "v2.2.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.2.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-rag-qa#v2.2.0-beta.1",
      },
      {
        version: "2.1.0",
        tag: "v2.1.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.1.0",
        install_source: "Locaryn/morph-rag-qa#v2.1.0",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-rag-qa#v2.0.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-rag-qa#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-ssh",
    name: "morph-ssh",
    display_name: "Connecteur SSH",
    description: "Espaces de travail distants et commandes SSH transparentes pour le chat.",
    author: "Locaryn Team",
    version: "2.2.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-ssh",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-ssh#v2.2.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.2.0-beta.1",
        tag: "v2.2.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.2.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-ssh#v2.2.0-beta.1",
      },
      {
        version: "2.1.0",
        tag: "v2.1.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.1.0",
        install_source: "Locaryn/morph-ssh#v2.1.0",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-ssh#v2.0.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-ssh#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-translation",
    name: "morph-translation",
    display_name: "Traduction IA",
    description: "Traduction automatique multilingue haute fidélité fonctionnant 100% hors-ligne.",
    author: "Locaryn Team",
    version: "2.1.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-translation",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-translation#v2.1.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.1.0-beta.1",
        tag: "v2.1.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.1.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-translation#v2.1.0-beta.1",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-translation#v2.0.0",
      },
      {
        version: "1.5.0",
        tag: "v1.5.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.5.0",
        install_source: "Locaryn/morph-translation#v1.5.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-translation#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-text-analysis",
    name: "morph-text-analysis",
    display_name: "Analyse de Texte",
    description: "Extraction d'entités, analyse de sentiment, classification et résumé de texte.",
    author: "Locaryn Team",
    version: "2.1.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-text-analysis",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-text-analysis#v2.1.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.1.0-beta.1",
        tag: "v2.1.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.1.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-text-analysis#v2.1.0-beta.1",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-text-analysis#v2.0.0",
      },
      {
        version: "1.5.0",
        tag: "v1.5.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.5.0",
        install_source: "Locaryn/morph-text-analysis#v1.5.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-text-analysis#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-model-training",
    name: "morph-model-training",
    display_name: "Entraînement & LoRA",
    description: "Atelier local de fine-tuning LoRA et oblitération de concepts / RepE.",
    author: "Locaryn Team",
    version: "2.1.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-model-training",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-model-training#v2.1.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.1.0-beta.1",
        tag: "v2.1.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.1.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-model-training#v2.1.0-beta.1",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-model-training#v2.0.0",
      },
      {
        version: "1.5.0",
        tag: "v1.5.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.5.0",
        install_source: "Locaryn/morph-model-training#v1.5.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-model-training#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-travel-tunnel",
    name: "morph-travel-tunnel",
    display_name: "Remote (Travel Mode)",
    description: "Tunnels chiffrés et appairage sécurisé pour contrôler Locaryn à distance.",
    author: "Locaryn Team",
    version: "2.2.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-travel-tunnel",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-travel-tunnel#v2.2.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.2.0-beta.1",
        tag: "v2.2.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.2.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-travel-tunnel#v2.2.0-beta.1",
      },
      {
        version: "2.1.0",
        tag: "v2.1.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.1.0",
        install_source: "Locaryn/morph-travel-tunnel#v2.1.0",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-travel-tunnel#v2.0.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-travel-tunnel#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-freetoken",
    name: "morph-freetoken",
    display_name: "FreeToken Optimizer",
    description: "Gestionnaire de quotas intelligents et optimisation de tokens gratuits.",
    author: "Locaryn Team",
    version: "2.1.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-freetoken",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-freetoken#v2.1.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "2.1.0-beta.1",
        tag: "v2.1.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (2.1.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-freetoken#v2.1.0-beta.1",
      },
      {
        version: "2.0.0",
        tag: "v2.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v2.0.0",
        install_source: "Locaryn/morph-freetoken#v2.0.0",
      },
      {
        version: "1.0.0",
        tag: "v1.0.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v1.0.0",
        install_source: "Locaryn/morph-freetoken#v1.0.0",
      },
    ],
  },
  {
    id: "locaryn:morph-omniroute",
    name: "morph-omniroute",
    display_name: "OmniRoute Gateway",
    description:
      "Passerelle OmniRoute : un point d'accès unifié pour des centaines de modèles distants.",
    author: "Locaryn Team",
    version: "1.0.0-beta.1",
    homepage: "https://github.com/Locaryn/morph-omniroute",
    ecosystem: "locaryn",
    catalog_id: "locaryn:official",
    catalog_label: "Locaryn Official",
    install_source: "Locaryn/morph-omniroute#v1.0.0-beta.1",
    keywords: ["official", "morph", "beta"],
    advertised: ["morph officiel", "bêta"],
    compat: "native",
    installed: false,
    is_beta: true,
    versions: [
      {
        version: "1.0.0-beta.1",
        tag: "v1.0.0-beta.1",
        is_beta: true,
        released_at: "2026-08-29",
        summary: "Version Bêta (1.0.0-beta.1) — pre-release non testée par des utilisateurs",
        install_source: "Locaryn/morph-omniroute#v1.0.0-beta.1",
      },
      {
        version: "0.9.0",
        tag: "v0.9.0",
        is_beta: false,
        released_at: "2026-08-27",
        summary: "Version de référence stable v0.9.0",
        install_source: "Locaryn/morph-omniroute#v0.9.0",
      },
    ],
  },

  {
    id: "claude_code:demo:commit-commands",
    name: "commit-commands",
    display_name: "commit-commands",
    description: "Commandes pour le flux de commit git.",
    author: "Anthropic",
    version: "1.0.0",
    homepage: null,
    ecosystem: "claude_code",
    catalog_id: "claude-code:anthropics/claude-code",
    catalog_label: "anthropics/claude-code",
    install_source: "github:anthropics/claude-code#plugins/commit-commands",
    keywords: ["git"],
    advertised: ["productivity"],
    compat: "adapted",
    installed: false,
  },
  {
    id: "gemini_cli:demo:flutter",
    name: "flutter",
    display_name: "flutter",
    description: "Outils Flutter et Dart pour l'agent.",
    author: "gemini-cli-extensions",
    version: "1.1.0",
    homepage: "https://github.com/gemini-cli-extensions/flutter",
    ecosystem: "gemini_cli",
    catalog_id: "gemini-cli:registry",
    catalog_label: "geminicli.com",
    install_source: "https://github.com/gemini-cli-extensions/flutter",
    keywords: [],
    advertised: ["mcp", "commands", "412 étoiles"],
    compat: "partial",
    installed: false,
  },
  {
    id: "mcp:demo:context7",
    name: "context7",
    display_name: "Context7",
    description: "Documentation à jour des bibliothèques, servie en MCP distant.",
    author: "upstash",
    version: "1.0.0",
    homepage: null,
    ecosystem: "mcp",
    catalog_id: "mcp:official",
    catalog_label: "registry.modelcontextprotocol.io",
    install_source: "mcp-remote:https://mcp.context7.com/mcp",
    keywords: [],
    advertised: ["remote"],
    compat: "native",
    installed: false,
  },
  {
    id: "opencode:demo:opencode-gitlab-auth",
    name: "opencode-gitlab-auth",
    display_name: "opencode-gitlab-auth",
    description: "Authentification OAuth GitLab pour OpenCode.",
    author: "npm",
    version: "2.1.0",
    homepage: "https://github.com/example/opencode-gitlab-auth",
    ecosystem: "opencode",
    catalog_id: "opencode:npm",
    catalog_label: "npm — opencode-plugin",
    install_source: "https://github.com/example/opencode-gitlab-auth",
    keywords: ["oauth"],
    advertised: ["113612/mois"],
    compat: "partial",
    installed: false,
  },
];

let demoSources: CatalogSource[] = [
  {
    id: "claude-code:anthropics/claude-code",
    label: "anthropics/claude-code",
    ecosystem: "claude_code",
    url: "https://raw.githubusercontent.com/anthropics/claude-code/HEAD/.claude-plugin/marketplace.json",
    builtin: true,
    enabled: true,
  },
  {
    id: "gemini-cli:registry",
    label: "geminicli.com",
    ecosystem: "gemini_cli",
    url: "https://geminicli.com/extensions.json",
    builtin: true,
    enabled: true,
  },
  {
    id: "mcp:official",
    label: "registry.modelcontextprotocol.io",
    ecosystem: "mcp",
    url: "https://registry.modelcontextprotocol.io/v0/servers",
    builtin: true,
    enabled: true,
  },
  {
    id: "opencode:npm",
    label: "npm — opencode-plugin",
    ecosystem: "opencode",
    url: "https://registry.npmjs.org/-/v1/search?text=keywords:opencode-plugin",
    builtin: true,
    enabled: true,
  },
];

const demoSourceStatuses: CatalogSourceStatus[] = demoSources.map((source) => ({
  source,
  ok: true,
  entry_count: demoCatalog.filter((e) => e.catalog_id === source.id).length,
  error: null,
}));

// Résidence simulée. Volontairement *mutable* : un mode démo qui répondrait
// toujours « chargé » ne permettrait pas de voir l'écran vide, l'animation de
// chargement ni le refus du garde-fou — c'est-à-dire tout ce qu'il y a à
// vérifier. La machine simulée a 32 Go dont 9 libres, ce qui suffit à faire
// refuser un gros modèle pour de vrai.
const demoMemory = { freeRamGb: 9.2, freeVramGb: 3.4 };
let demoResident: { model: string | null; pinned: boolean; since: number } = {
  model: null,
  pinned: false,
  since: Date.now(),
};
let demoCaution: CautionLevel = "equilibre";

/** Paramètres déduits du nom du fichier, faute de disque à mesurer. */
function demoParamsB(model: string): number {
  const m = /(\d+(?:[.,]\d+)?)\s*b\b/i.exec(model);
  return m ? Math.max(0.5, Number.parseFloat(m[1].replace(",", "."))) : 7;
}

/** La machine du mode démonstration : un portable milieu de gamme. */
const demoHardware: LlmfitHardware = {
  cpu_cores: 8,
  total_ram_gb: 16,
  free_ram_gb: demoMemory.freeRamGb,
  gpu_name: "NVIDIA GeForce RTX 4060 Laptop GPU",
  total_vram_gb: 8,
  free_vram_gb: demoMemory.freeVramGb,
  backend: "cuda",
  ram_bandwidth_gbps: 48,
  vram_bandwidth_gbps: 272,
  ram_bandwidth_measured: false,
  unified_memory: false,
};

/**
 * L'estimation, version démonstration.
 *
 * Reprend la structure du calcul natif — poids, cache d'attention, tampons —
 * pour que l'interface montre les mêmes cas de figure sans moteur Rust. Les
 * nombres sont plausibles, pas mesurés.
 */
function demoFit(model: string, level: CautionLevel, paramsB?: number): ModelFit {
  const billions = paramsB ?? demoParamsB(model);
  const weights = billions * 0.6;
  const context = 8192;
  // Le gabarit d'un transformeur moderne : 8 têtes de clé de 128, en f16.
  const layers = Math.max(16, Math.round(24 + billions));
  const kv = (2 * layers * context * 8 * 128 * 2) / 1024 ** 3;
  const compute = 0.4;
  const [reserveVram, reserveRam] =
    level === "prudent" ? [1.5, 3.0] : level === "equilibre" ? [0.6, 1.5] : [0.0, 0.0];
  const required = weights + kv + compute;
  const usableVram = Math.max(0, demoHardware.free_vram_gb - reserveVram);
  const usableRam = Math.max(0, demoHardware.free_ram_gb - reserveRam);

  const perLayer = (weights + kv) / layers;
  const gpuLayers = Math.min(layers, Math.max(0, Math.floor((usableVram - compute) / perLayer)));
  const onGpu = required <= usableVram;
  const inRam = required - gpuLayers * perLayer <= usableRam;
  const speed = onGpu
    ? (demoHardware.vram_bandwidth_gbps * 0.85) / Math.max(weights, 0.3)
    : (demoHardware.ram_bandwidth_gbps * 0.6) / Math.max(weights, 0.3);

  const base = {
    model,
    size_gb: weights,
    kv_cache_gb: kv,
    compute_gb: compute,
    required_gb: required,
    free_ram_gb: demoHardware.free_ram_gb,
    free_vram_gb: demoHardware.free_vram_gb,
    level,
    context,
    total_layers: layers,
    prompt_tokens_per_second: onGpu ? 900 : 40,
    max_gpu_context: onGpu ? context : 0,
    max_context: inRam ? context : 2048,
    quant: "Q4_K_M",
    estimated: paramsB !== undefined,
    assumptions: [
      `Cache d'attention en f16, contexte de ${context} jetons, lot de 512.`,
      "Bande passante mémoire non mesurable en démonstration, 48 Go/s supposés.",
      "Vitesse déduite de la bande passante mémoire, cache à moitié plein.",
    ],
  };

  if (onGpu)
    return {
      ...base,
      verdict: "confortable",
      placement: "gpu",
      gpu_layers: layers,
      tokens_per_second: speed,
      suggested_quant: null,
      overridable: false,
      message: `${(weights + kv).toFixed(1)} Go entièrement sur le GPU (${demoHardware.free_vram_gb.toFixed(1)} Go libres), contexte de ${context} jetons. Environ ${Math.round(speed)} jetons/s.`,
    };
  if (inRam)
    return {
      ...base,
      verdict: "juste",
      placement: gpuLayers > 0 ? "partage" : "ram",
      gpu_layers: gpuLayers,
      tokens_per_second: speed,
      suggested_quant: "Q3_K_M",
      overridable: false,
      message:
        gpuLayers > 0
          ? `${gpuLayers} couches sur ${layers} tiennent dans les ${demoHardware.free_vram_gb.toFixed(1)} Go de VRAM libres, le reste passe par la RAM. Environ ${speed.toFixed(1)} jetons/s.`
          : `${(weights + kv).toFixed(1)} Go en RAM (${demoHardware.free_ram_gb.toFixed(1)} Go libres). Environ ${speed.toFixed(1)} jetons/s.`,
    };
  if (level === "risque")
    return {
      ...base,
      verdict: "risque",
      placement: "disque",
      gpu_layers: gpuLayers,
      tokens_per_second: 0.4,
      suggested_quant: "Q3_K_M",
      overridable: false,
      message: `${required.toFixed(1)} Go nécessaires pour ${demoHardware.free_ram_gb.toFixed(1)} Go libres. Le système compensera sur le disque : ralentissement sévère, et l'application peut être tuée par manque de mémoire.`,
    };
  return {
    ...base,
    verdict: "refuse",
    placement: "disque",
    gpu_layers: gpuLayers,
    tokens_per_second: 0.4,
    suggested_quant: "Q3_K_M",
    overridable: true,
    message: `${required.toFixed(1)} Go nécessaires, ${demoHardware.free_ram_gb.toFixed(1)} Go libres en RAM et ${demoHardware.free_vram_gb.toFixed(1)} Go en VRAM. Refusé au niveau de prudence choisi. La version Q3_K_M tiendrait.`,
  };
}

function demoResidencyStatus(): ResidencyStatus {
  return {
    loaded: demoResident.model !== null,
    model: demoResident.model,
    pinned: demoResident.pinned,
    idle_seconds: demoResident.model
      ? Math.min(1800, Math.floor((Date.now() - demoResident.since) / 1000))
      : 0,
    idle_timeout_seconds: 1800,
    endpoint: demoResident.model ? "http://127.0.0.1:8080" : null,
  };
}

function demoMemoryEntry(
  group: MemoryGroup,
  title: string,
  summary: string,
  details: string[],
): MemoryEntry {
  return {
    id: `demo-mem-${title.toLowerCase().replace(/\s+/g, "-")}`,
    user_id: null,
    group,
    title,
    summary,
    details,
    source: "assistant",
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
}

let demoUserMemory: MemoryEntry[] = [
  demoMemoryEntry("vous", "Préférences", "Répond en français, sans préambule.", [
    "Répond en français, sans préambule.",
  ]),
  demoMemoryEntry("sujets", "Coding Projects", "Projets de code personnels et scolaires.", [
    "Projets de code personnels et scolaires.",
  ]),
  demoMemoryEntry("zones", "Bot Bastet", "Robot compagnon de campus.", [
    "Robot compagnon de campus, équipe de trois personnes.",
    "Pile : ROS2, YOLOv8, reconnaissance faciale, LM Studio, FastAPI, WebSockets.",
  ]),
  demoMemoryEntry("personnes", "Paul", "Coéquipier sur le projet Bot Bastet.", [
    "Coéquipier sur le projet Bot Bastet.",
  ]),
];

const demoCore: CoreApi = {
  health: async () => cloneHealth(),
  bootstrap: async () => ({
    project: demoProject,
    session: demoSessions[0],
    health: cloneHealth(),
  }),
  listProjects: async () => [demoProject, demoProject2],
  createProject: async (path, name) => ({
    ...demoProject,
    id: `demo-project-${Math.random().toString(36).slice(2, 8)}`,
    path,
    name,
    trust_level: "untrusted",
  }),
  updateProject: async (id, name, trustLevel) => ({
    ...demoProject,
    id,
    name: name ?? demoProject.name,
    trust_level: trustLevel ?? demoProject.trust_level,
  }),
  archiveProject: async () => {},
  freeChatProject: async () => ({
    ...demoProject,
    id: "demo-free",
    path: "__locaryn_free_chats__",
    name: "Conversations libres",
  }),
  sessionWorkspace: async () => "/tmp/locaryn-demo",
  appendChatMessage: async () => {},
  appendAssistantMessage: async () => {},
  planTask: async (request) => ({
    needs_plan: /cr[ée]e|d[ée]veloppe|impl[ée]mente|corrige|refactor/i.test(request),
    needs_loop: /corrige|fix|marche pas|bug/i.test(request),
    steps: [
      "Analyser le code existant",
      "Appliquer la modification",
      "Vérifier que tout fonctionne",
    ],
  }),
  suggestFollowups: async () => [
    "Lier le bouton à une fonction",
    "Vérifier le CSS associé",
    "Ajouter un test pour ce composant",
  ],
  getImageDefaults: async () => demoImageDefaults,
  setImageDefaults: async (config) => {
    // Stateful like the real backend, so the settings UI behaves identically.
    demoImageDefaults = { ...config };
  },
  getModelPreferences: async () => demoModelPreferences,
  setModelPreferences: async (preferences) => {
    demoModelPreferences = { ...preferences };
  },
  getLocalProfile: async () => demoLocalProfile,
  setLocalProfile: async (displayName) => {
    demoLocalProfile = { ...demoLocalProfile, display_name: displayName.trim().slice(0, 80) };
    return demoLocalProfile;
  },
  setLocalAvatar: async (sourcePath) => {
    demoLocalProfile = { ...demoLocalProfile, avatar_path: sourcePath };
    return demoLocalProfile;
  },
  clearLocalAvatar: async () => {
    demoLocalProfile = { ...demoLocalProfile, avatar_path: null };
    return demoLocalProfile;
  },
  planModelRuntime: async (model) => ({
    model,
    size_gb: 4.2,
    vram_gb: 6,
    ram_gb: 32,
    mode: "gpu",
    label: "",
    gpu_layers: -1,
    n_cpu_moe: 0,
  }),
  listSessions: async (projectId) => demoSessions.filter((s) => s.project_id === projectId),
  createSession: async (projectId, title, coreId) => {
    demoSessionCounter += 1;
    const s: Session = {
      id: `demo-session-${demoSessionCounter}`,
      project_id: projectId,
      title: title ?? null,
      provider_id: null,
      model: null,
      created_at: new Date().toISOString(),
      last_message_at: null,
      closed_at: null,
      core_id: coreId ?? null,
    };
    demoSessions.push(s);
    return s;
  },
  updateSessionTitle: async () => {},
  generateSessionTitle: async (_sessionId, firstPrompt) => {
    const words = firstPrompt.trim().split(/\s+/).slice(0, 5);
    return words.join(" ").replace(/[.!?\n]+$/, "");
  },
  deleteSession: async () => {},
  archiveSession: async () => {},
  archivedSessions: async () => [],
  runComposerTool: async (_tool: string, text: string) => text,
  invokeExtensionTool: async (tool: string, _args: Record<string, unknown>) => {
    // La démo répond comme le ferait le serveur MCP d'une extension : un objet
    // JSON encodé en texte. Sans cela, tout écran qui interroge une extension
    // reste vide hors Tauri, sans dire pourquoi.
    if (tool === "list_image_models") {
      return JSON.stringify({
        models: [
          "z_image_turbo-Q8_0.gguf",
          "flux1-schnell-Q4_0.gguf",
          "sd_xl_turbo_1.0.q8_0.gguf",
          "stable-diffusion-v1-5-pruned-emaonly-Q4_0.gguf",
        ],
      });
    }
    return "{}";
  },
  moveSession: async () => {},
  suggestProject: async () => ({ project_id: null }),
  mergeSessions: async () => {},
  renameSession: async () => {},
  listFigures: async () => [],
  saveFigure: async (f) => ({
    id: "demo",
    name: f.name,
    description: f.description,
    instructions: f.instructions,
    model: f.model,
    opening: f.opening,
    uses_memory: f.usesMemory,
    tools: f.tools
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean),
    source: "user",
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  }),
  deleteFigure: async () => {},
  attachFigure: async () => {},
  figureSessions: async () => [],
  createEphemeralSession: async (projectId) => ({
    id: "ephemere",
    project_id: projectId,
    title: null,
    provider_id: null,
    model: null,
    created_at: new Date().toISOString(),
    last_message_at: null,
    closed_at: null,
    archived_at: null,
    ephemeral: true,
  }),
  listMessages: async (sessionId) => demoMessages.filter((m) => m.session_id === sessionId),

  async sendMessage(_sessionId, content, onEvent, images, _responseFormat, _reasoning) {
    onEvent({ type: "message_start", message_id: "demo", task_id: "demo" });
    // Reasoning models stream a scratchpad before the answer; emit one so the
    // collapsing behaviour can be developed without a local model loaded.
    for (const chunk of [
      "<think>L'utilisateur demande d'inspecter le projet. ",
      "Je dois d'abord lire le fichier principal pour comprendre la structure, ",
      "puis résumer ce que fait le programme sans entrer dans les détails.\n",
      "Attention à ne pas inventer de dépendances qui ne sont pas dans le code.</think>",
    ]) {
      await sleep(180);
      onEvent({ type: "token", text: chunk });
    }
    if (images && images.length > 0) {
      await sleep(200);
      onEvent({
        type: "token",
        text: `(demo) received ${images.length} image${images.length === 1 ? "" : "s"}. A vision model would describe them here.\n\n`,
      });
    }
    await sleep(350);
    onEvent({
      type: "tool_call",
      call_id: "c1",
      tool: "read_file",
      args: { path: "src/main.rs" },
    });
    await sleep(700);
    onEvent({
      type: "tool_result",
      call_id: "c1",
      ok: true,
      output: 'fn main() {\n    println!("hello locaryn");\n}\n',
    });
    // Un appel qui exige un accord. Sans lui, la fenêtre d'approbation ne
    // serait atteignable qu'avec un vrai modèle branché — c'est-à-dire
    // jamais pendant le développement de l'interface.
    await sleep(400);
    onEvent({
      type: "tool_approval",
      call_id: "c2",
      tool: "write_file",
      args: { path: "src/main.rs", contents: "// modifié par la démo\n" },
      risk: "high",
      reason: "Cet outil écrit dans un fichier du projet",
      diff: [
        "--- a/src/main.rs",
        "+++ b/src/main.rs",
        "@@",
        '-    println!("hello locaryn");',
        '+    println!("bonjour locaryn");',
      ].join("\n"),
      is_remote: false,
    });
    await sleep(400);
    const reply = `Demo mode — no Rust core attached. You said:\n\n> ${content}\n\nHere is a *markdown* sample with \`inline code\` and a block:\n\n\`\`\`ts\nconst answer = 42;\nexport default answer;\n\`\`\`\n\n1. First step\n2. Second step`;
    for (const word of reply.split(/(?<=\s)/)) {
      onEvent({ type: "token", text: word });
      await sleep(18);
    }
    onEvent({
      type: "message_end",
      message_id: "demo",
      tokens_in: 120,
      tokens_out: 60,
      duration_ms: 1800,
    });
  },

  async runTerminal(command, _cwd, onOutput) {
    await sleep(150);
    onOutput({ type: "line", stream: "stdout", text: `demo shell: ${command}` });
    await sleep(120);
    onOutput({ type: "line", stream: "stderr", text: "(no real shell in browser demo)" });
    onOutput({ type: "exit", code: 0 });
  },

  listProviders: async () => demoProviders,
  setActiveProvider: async (id) => {
    demoProviders = demoProviders.map((p) => ({ ...p, is_active: p.id === id }));
    return demoProviders.find((p) => p.id === id) ?? demoProviders[0];
  },
  configureProvider: async (endpoint, model) => {
    await sleep(200);
    const p: Provider = {
      ...demoProviders[0],
      endpoint,
      model,
      is_active: true,
      status: "healthy",
      updated_at: new Date().toISOString(),
    };
    demoProviders = [p];
    // Reflect into health so the TopBar updates in demo mode.
    demoHealth.active_provider = { kind: "local", engine: "ollama", endpoint, model };
    return p;
  },
  listModels: async (endpoint) => {
    await sleep(300);
    const local = /(127\.0\.0\.1|localhost|11434)/.test(endpoint);
    if (!local) {
      throw new Error(`cannot reach ${endpoint}: demo only serves the default endpoint`);
    }
    return demoModels;
  },
  listIncompatibleModels: async () => [],
  listNonChatModels: async () => [],
  airllmStatus: async () => ({
    python: true,
    pythonPath: "demo-python",
    torch: true,
    airllmInstalled: true,
    installed: [],
  }),
  airllmSetup: async (onLine) => {
    await sleep(400);
    onLine?.("[demo] environnement AirLLM prêt");
  },
  airllmInstall: async (repo, onLine) => {
    await sleep(600);
    onLine?.("[demo] poids AirLLM téléchargés");
    return { repo, installedAt: new Date().toISOString() };
  },
  airllmInstalled: async () => [],
  airllmUninstall: async () => {},
  configureAirllmProvider: async (repo) => {
    await sleep(200);
    const p: Provider = {
      ...demoProviders[0],
      id: `airllm-${repo.replace(/[^a-z0-9]/gi, "-")}`,
      engine: "airllm",
      endpoint: "http://127.0.0.1:8337/v1",
      model: repo,
      is_active: true,
      status: "healthy",
      config: { repo },
      updated_at: new Date().toISOString(),
    };
    demoProviders = [p];
    demoHealth.active_provider = {
      kind: "local",
      engine: "airllm",
      endpoint: p.endpoint,
      model: repo,
    };
    return p;
  },
  inspectHuggingFaceRepo: async (source) => ({
    repo: source.replace(/^https?:\/\/huggingface\.co\//, "").replace(/^hf\.co\//, ""),
    candidates: [
      {
        id: "demo-q4",
        label: "Model Instruct — Q4_K_M",
        files: ["model-Q4_K_M.gguf"],
        support_files: ["mmproj-model-Q8_0.gguf"],
        total_bytes: 4_200_000_000,
        format: "gguf",
        quantization: "Q4_K_M",
        variant: "Model Instruct",
      },
      {
        id: "demo-q8",
        label: "Model Instruct — Q8_0",
        files: ["model-Q8_0.gguf"],
        support_files: ["mmproj-model-Q8_0.gguf"],
        total_bytes: 7_900_000_000,
        format: "gguf",
        quantization: "Q8_0",
        variant: "Model Instruct",
      },
    ],
    support_files: ["config.json", "tokenizer.json"],
    total_bytes: 12_100_000_000,
    warning: null,
    suggested_repo: null,
  }),
  pullModel: async (_endpoint, model, onProgress, _heretic, consent) => {
    if (
      /nsfw|uncensored|pony|urpm|realisticvision|abyssorangemix|counterfeit|flux.*uncensored|hunyuanvideo.*nsfw|wan2.*nsfw/i.test(
        model,
      ) &&
      !consent
    ) {
      throw new Error(
        "Ce modèle est classé NSFW / sans garde-fous. Acceptez la responsabilité avant de télécharger. [demo]",
      );
    }
    for (let i = 20; i <= 100; i += 20) {
      await sleep(200);
      onProgress?.(i, `Téléchargement de ${model}... ${i}%`);
    }
    if (!demoModels.includes(model)) {
      demoModels.push(model);
    }
  },
  cancelPullModel: async () => {
    await sleep(100);
  },
  deleteModel: async (_endpoint, model) => {
    await sleep(200);
    const idx = demoModels.indexOf(model);
    if (idx !== -1) {
      demoModels.splice(idx, 1);
    }
  },
  appInfo: async () => ({
    version: "0.1.0-demo",
    mode: "local",
    data_dir: "C:/Users/you/.locaryn/data",
    db_path: "C:/Users/you/.locaryn/data/locaryn.db",
    models_dir: "C:/Users/you/.locaryn/data/models",
    // Le navigateur ne connaît pas la machine : on rend ce qu'il sait, plutôt
    // qu'une valeur inventée qui masquerait un écran mal adapté.
    platform: navigator.platform || "navigateur",
    arch: "inconnue",
  }),

  listVoicePresets: async () => [
    {
      id: "demo-1",
      name: "Ma petite soeur",
      note: "voix douce, débit rapide",
      referenceAudio: "C:/Users/you/.locaryn/voice_presets/demo-1/reference.wav",
      referenceText: "et ça m'énerve genre pendant le chargement là tu vois",
      language: "fr",
      durationS: 12,
      settings: { ...VOICE_SETTINGS_DEFAULTS, temperature: 0.95, pauseScale: 0.9 },
      engine: "Qwen3-TTS",
      createdAt: "2026-07-31T10:00:00Z",
      updatedAt: "",
    },
  ],
  saveVoicePreset: async (args) => ({
    id: args.id ?? "demo-new",
    name: args.name,
    note: args.note ?? "",
    referenceAudio: args.referenceAudio ?? "",
    referenceText: args.referenceText ?? "",
    language: args.language ?? "fr",
    durationS: 12,
    settings: args.settings,
    engine: args.engine ?? "",
    createdAt: "2026-07-31T10:00:00Z",
    updatedAt: "",
  }),
  deleteVoicePreset: async () => {},
  voicePresetSupport: async (model) => ({
    engine: "Qwen3-TTS",
    cloning: model.toLowerCase().includes("base"),
    referenceText: model.toLowerCase().includes("base"),
    temperature: true,
    speed: true,
    pitch: true,
    pauseScale: true,
    instruct: true,
  }),

  serverStatus: async () => ({
    running: false,
    bind: "0.0.0.0",
    port: 7474,
    url: "",
    accounts: 0,
    fingerprint: null,
    blocker:
      "Aucun compte n'existe. Un serveur accessible sans compte serait ouvert à tous : créez d'abord un administrateur.",
  }),
  setServerMode: async (enabled) => ({
    running: enabled,
    bind: "0.0.0.0",
    port: 7474,
    url: enabled ? "https://192.168.1.188:7474" : "",
    accounts: 1,
    fingerprint: enabled ? "BD:E9:FA:13:1A:62:B6:93" : null,
    blocker: null,
  }),
  restartServer: async () => ({
    running: true,
    bind: "0.0.0.0",
    port: 7474,
    url: "https://192.168.1.188:7474",
    accounts: 1,
    fingerprint: "BD:E9:FA:13:1A:62:B6:93",
    blocker: null,
  }),
  listServerUsers: async () => [
    { id: "usr-admin-1", username: "admin", role: "admin", disabled: false },
  ],
  createServerUser: async (username, _password, isAdmin = true) => ({
    running: false,
    bind: "0.0.0.0",
    port: 7474,
    url: "",
    accounts: 1,
    fingerprint: null,
    blocker: null,
  }),
  deleteServerUser: async () => ({
    running: false,
    bind: "0.0.0.0",
    port: 7474,
    url: "",
    accounts: 0,
    fingerprint: null,
    blocker: "Aucun compte n'existe.",
  }),
  provisioning: async () => null,

  signIn: async (serverUrl, username) => ({
    server_url: serverUrl,
    username,
    token: "demo",
  }),
  currentSession: async () => null,
  signOut: async () => {},

  clientCertificateStatus: async () => ({
    installed: false,
    issued_to: null,
    path: null,
    authority_installed: false,
  }),
  installClientCertificate: async () => ({
    installed: true,
    issued_to: "demo",
    path: "C:/…/client-tls/client.pem",
    authority_installed: false,
  }),
  removeClientCertificate: async () => ({
    installed: false,
    issued_to: null,
    path: null,
    authority_installed: false,
  }),

  storageInfo: async () => ({
    root: "C:/Users/you/.locaryn/data",
    configured: false,
    total_bytes: 41_231_686_042,
    entries: [
      {
        key: "models",
        label: "Modèles (poids)",
        path: "C:/Users/you/.locaryn/data/models",
        size_bytes: 41_284_378_624,
        exists: true,
        outside_root: false,
      },
      {
        key: "bin",
        label: "Moteurs (llama.cpp, sd.cpp)",
        path: "C:/Users/you/.locaryn/data/bin",
        size_bytes: 322_961_408,
        exists: true,
        outside_root: false,
      },
      {
        key: "tmp",
        label: "Fichiers temporaires",
        path: "C:/Users/you/.locaryn/data/tmp",
        size_bytes: 0,
        exists: false,
        outside_root: false,
      },
      {
        key: "free_chats",
        label: "Pièces jointes des chats",
        path: "C:/Users/you/.locaryn/data/free_chats",
        size_bytes: 12_582_912,
        exists: true,
        outside_root: false,
      },
    ],
    db_path: "C:/Users/you/.locaryn/data/locaryn.db",
    db_bytes: 4_194_304,
    drives: [
      { mount: "C:\\", total_bytes: 511_000_000_000, free_bytes: 1_288_490_188, is_current: true },
      {
        mount: "D:\\",
        total_bytes: 1_000_000_000_000,
        free_bytes: 56_489_000_000,
        is_current: false,
      },
    ],
  }),
  setStorageRoot: async (newRoot) => ({
    root: newRoot,
    configured: true,
    total_bytes: 0,
    entries: [],
    drives: [],
    db_path: "C:/Users/you/.locaryn/data/locaryn.db",
    db_bytes: 4_194_304,
  }),
  cleanTemp: async () => 8_589_934_592,

  listConnectorTypes: async () => demoConnectorTypes,

  // Mémoire de démonstration : de vraies entrées, pour que l'écran se
  // travaille dans un navigateur sans base derrière.
  listModelMetrics: async () => [
    {
      model: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
      kind: "chat",
      samples: 12,
      avg_tokens_per_second: 35.1,
      avg_duration_ms: null,
      last_measured_at: new Date().toISOString(),
    },
  ],
  listMemory: async () => demoUserMemory,
  remember: async (group, title, detail) => {
    const existante = demoUserMemory.find(
      (m) => m.group === group && m.title.toLowerCase() === title.toLowerCase(),
    );
    if (existante) {
      if (detail && !existante.details.some((d) => d.toLowerCase() === detail.toLowerCase())) {
        existante.details = [...existante.details, detail];
      }
      existante.updated_at = new Date().toISOString();
      return existante;
    }
    const entry = demoMemoryEntry(group, title, detail || title, detail ? [detail] : []);
    demoUserMemory = [entry, ...demoUserMemory];
    return entry;
  },
  setMemorySummary: async (id, summary) => {
    demoUserMemory = demoUserMemory.map((m) =>
      m.id === id ? { ...m, summary, updated_at: new Date().toISOString() } : m,
    );
    return demoUserMemory.find((m) => m.id === id) as MemoryEntry;
  },
  renameMemoryEntry: async (id, title) => {
    demoUserMemory = demoUserMemory.map((m) =>
      m.id === id ? { ...m, title, updated_at: new Date().toISOString() } : m,
    );
    return demoUserMemory.find((m) => m.id === id) as MemoryEntry;
  },
  setMemoryGroup: async (id, group) => {
    demoUserMemory = demoUserMemory.map((m) =>
      m.id === id ? { ...m, group, updated_at: new Date().toISOString() } : m,
    );
    return demoUserMemory.find((m) => m.id === id) as MemoryEntry;
  },
  removeMemoryDetail: async (id, detail) => {
    demoUserMemory = demoUserMemory.map((m) =>
      m.id === id
        ? {
            ...m,
            details: m.details.filter((d) => d !== detail),
            updated_at: new Date().toISOString(),
          }
        : m,
    );
    return demoUserMemory.find((m) => m.id === id) as MemoryEntry;
  },
  forgetMemory: async (id) => {
    demoUserMemory = demoUserMemory.filter((m) => m.id !== id);
  },
  forgetAllMemory: async () => {
    const n = demoUserMemory.length;
    demoUserMemory = [];
    return n;
  },
  runMemoryCommand: async () => ({
    summary: "Mode aperçu navigateur : la boîte de commande a besoin d'un moteur actif.",
    applied: 0,
    entries: demoUserMemory,
  }),
  listExtensions: async () => demoExtensions,
  listCapabilities: async () => [],
  coreStatus: async (id) => ({
    id,
    state: "running",
    driver: "responses",
    api_url: "http://127.0.0.1:18789/v1/responses",
  }),
  coreStart: async (id) => ({
    id,
    state: "running",
    driver: "responses",
    api_url: "http://127.0.0.1:18789/v1/responses",
  }),
  coreStop: async (id) => ({
    id,
    state: "stopped",
    driver: "responses",
    api_url: "http://127.0.0.1:18789/v1/responses",
  }),
  coreSkills: async () => [
    {
      slug: "home-assistant",
      name: "Home Assistant",
      description: "Contrôle de Home Assistant : états, scènes, automatisations.",
      verified: true,
    },
    {
      slug: "calendar",
      name: "Calendrier",
      description: "Lecture et gestion d'agendas.",
      verified: false,
    },
    {
      slug: "web-search",
      name: "Recherche web",
      description: "Recherche sur le web et extraction de contenu.",
      verified: false,
    },
  ],
  coreInstallSkill: async (_id, slug) => `skill « ${slug} » installé (démo)`,
  getExtensionMcpServers: async () => [],
  setExtensionMcpServers: async () => [],
  installExtension: async (source) => {
    const entry = demoCatalog.find((c) => c.install_source === source);
    const created: InstalledExtension = {
      id: `demo-${demoExtensions.length + 1}`,
      name: entry?.name ?? "extension-demo",
      display_name: entry?.display_name ?? "Extension démo",
      version: entry?.version ?? "1.0.0",
      api_version: "0.1",
      description: entry?.description ?? null,
      author: entry?.author ?? null,
      homepage: entry?.homepage ?? null,
      kind: "plugin",
      scope: "user",
      ecosystem: entry?.ecosystem ?? "locaryn",
      source,
      install_dir: `~/.locaryn/plugins/${entry?.name ?? "extension-demo"}`,
      enabled: false,
      components: {
        skills: 1,
        commands: 2,
        agents: 0,
        rules: 1,
        hooks: 0,
        mcp_servers: 1,
        lsp_adapters: 0,
        figures: 0,
      },
      permissions: [
        { permission: "mcp", reason: "Lancer son serveur MCP", granted: false },
        { permission: "files_read", reason: "Lire le projet", granted: false },
      ],
      load_errors: [],
      capabilities: [],
      ui: { nav_items: [], studio_tabs: [] },
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };
    demoExtensions = [...demoExtensions, created];
    if (entry) entry.installed = true;
    return created;
  },
  setExtensionEnabled: async (id, enabled) => {
    demoExtensions = demoExtensions.map((e) => (e.id === id ? { ...e, enabled } : e));
    return demoExtensions;
  },
  setExtensionPermissions: async (id, granted) => {
    demoExtensions = demoExtensions.map((e) =>
      e.id === id
        ? {
            ...e,
            permissions: e.permissions.map((p) => ({
              ...p,
              granted: granted.includes(p.permission),
            })),
          }
        : e,
    );
    return demoExtensions;
  },
  updateExtension: async (id) => {
    const existing = demoExtensions.find((e) => e.id === id);
    if (!existing) throw new Error("extension introuvable");
    const [major, ...rest] = existing.version.split(".");
    const version = `${(Number(major) || 0) + 1}.${rest.join(".") || "0"}`;
    const updated = { ...existing, version, updated_at: new Date().toISOString() };
    demoExtensions = demoExtensions.map((e) => (e.id === id ? updated : e));
    return updated;
  },
  updateExtensionSource: async (id, source) => {
    const existing = demoExtensions.find((e) => e.id === id);
    if (!existing) throw new Error("extension introuvable");
    const [major, ...rest] = existing.version.split(".");
    const version = `${(Number(major) || 0) + 1}.${rest.join(".") || "0"}`;
    const updated = { ...existing, source, version, updated_at: new Date().toISOString() };
    demoExtensions = demoExtensions.map((e) => (e.id === id ? updated : e));
    return updated;
  },
  checkExtensionUpdates: async () => [],
  reloadExtensions: async () => demoExtensions,
  previewExtensionSource: async (source) => ({
    manifest_file: "morph.json",
    ecosystem: "claude_code",
    name: source.split("/").pop() ?? "demo-plugin",
    version: "1.2.0",
    description: "Plugin de démonstration — aperçu avant installation.",
    author: "démo",
    requested_permissions: ["shell", "files_read"],
    mcp_servers: [
      {
        name: "demo-fs",
        command: "npx -y @modelcontextprotocol/server-filesystem",
        url: null,
      },
      { name: "demo-web", command: null, url: "https://demo.example.com/mcp" },
    ],
  }),
  removeExtension: async (id) => {
    demoExtensions = demoExtensions.filter((e) => e.id !== id);
    return demoExtensions;
  },
  getExtensionConfig: async (id) => demoExtensionConfigs[id] ?? { schema: null, values: {} },
  setExtensionConfig: async (id, values) => {
    const current = demoExtensionConfigs[id] ?? { schema: null, values: {} };
    const next = { ...current, values: { ...current.values, ...values } };
    demoExtensionConfigs[id] = next;
    return next;
  },
  listExtensionCommands: async () =>
    demoExtensions
      .filter((e) => e.enabled)
      .flatMap((e) => [
        { name: `${e.name}:review`, plugin: e.name, description: "Relire le diff", arguments: [] },
      ]),
  resolveExtensionCommand: async (name, args) => `[${name}] ${args}`.trim(),
  readExtensionAsset: async (_extensionId, _assetPath) => "",
  refreshExtensionAsset: async (_extensionId, _assetPath) => "",
  browseExtensionCatalog: async (opts) => {
    const q = (opts?.query ?? "").toLowerCase();
    const entries = demoCatalog
      .filter((e) => !opts?.ecosystem || e.ecosystem === opts.ecosystem)
      .filter(
        (e) =>
          !q ||
          e.display_name.toLowerCase().includes(q) ||
          (e.description ?? "").toLowerCase().includes(q),
      )
      .slice(0, opts?.limit ?? 60);
    return {
      entries,
      sources: demoSourceStatuses,
      fetched_at: new Date().toISOString(),
      stale: false,
    };
  },
  refreshExtensionCatalog: async () => {
    await sleep(600);
    return {
      entries: demoCatalog,
      sources: demoSourceStatuses,
      fetched_at: new Date().toISOString(),
      stale: false,
    };
  },
  listCatalogSources: async () => demoSources,
  addCatalogSource: async (spec) => {
    const [owner, repo] = spec.replace(/^https?:\/\/github\.com\//, "").split("/");
    demoSources = [
      ...demoSources,
      {
        id: `claude-code:${owner}/${repo}`,
        label: `${owner}/${repo}`,
        ecosystem: "claude_code",
        url: `https://raw.githubusercontent.com/${owner}/${repo}/HEAD/.claude-plugin/marketplace.json`,
        builtin: false,
        enabled: true,
      },
    ];
    return demoSources;
  },
  setCatalogSourceEnabled: async (id, enabled) => {
    demoSources = demoSources.map((s) => (s.id === id ? { ...s, enabled } : s));
    return demoSources;
  },
  removeCatalogSource: async (id) => {
    demoSources = demoSources.filter((s) => s.id !== id || s.builtin);
    return demoSources;
  },

  travelStatus: async () => ({
    active: false,
    provider: null,
    link: null,
    qr_svg: null,
    blocker: "Le partage réseau n'est pas actif. [démo]",
  }),
  travelRelays: async () => [
    {
      id: "cloudflare",
      label: "Cloudflare",
      installed: false,
      needs_account: false,
      install_hint:
        "Installez cloudflared (winget install Cloudflare.cloudflared). Aucun compte requis.",
    },
    {
      id: "ngrok",
      label: "ngrok",
      installed: false,
      needs_account: true,
      install_hint:
        "Installez ngrok depuis ngrok.com, puis « ngrok config add-authtoken <jeton> ».",
    },
    {
      id: "devtunnel",
      label: "Tunnels Microsoft",
      installed: false,
      needs_account: true,
      install_hint: "Installez devtunnel, puis « devtunnel user login ».",
    },
  ],
  setTravelMode: async () => ({
    active: false,
    provider: null,
    link: null,
    qr_svg: null,
    blocker: null,
  }),
  travelHomeCode: async () => ({
    active: false,
    provider: null,
    link: null,
    qr_svg: null,
    blocker: null,
  }),
  pairingCode: async (mode) => ({
    mode,
    url: mode === "local" ? "http://192.168.1.20:7474" : "https://exemple.invalide:7474",
    qr_svg: "",
  }),
  systemPrompt: async () => ({ texte: null, envoye: "" }),
  setSystemPrompt: async (texte) => ({ texte, envoye: texte ?? "" }),
  listDebridedModels: async () => [],
  toggleModelDebridage: async () => [],
  microModel: async () => ({ model: null, available: ["Qwen3-1.7B-Q4_K_M.gguf"] }),
  setMicroModel: async (model) => ({ model, available: ["Qwen3-1.7B-Q4_K_M.gguf"] }),

  listMcpServers: async () => [],
  addMcpServer: async () => [],
  removeMcpServer: async () => [],
  startMcpServer: async () => [
    "get_conversations",
    "get_messages",
    "send_message",
    "send_snap",
    "send_voice_note",
    "list_friends",
    "voice_call",
    "end_call",
    "call_status",
    "active_call",
  ],
  stopMcpServer: async () => {},
  invokeMcpTool: async (_name, tool, args) => ({
    demo: true,
    tool,
    arguments: args,
    message: `Outil ${tool} simulé en mode navigateur. Lancez Locaryn Tauri pour un compte réel.`,
  }),
  // Aucun SDK Android n'est joignable depuis un navigateur : la démo le dit
  // plutôt que d'inventer un émulateur qui n'existe pas.
  diagnoseAndroidVm: async () => ({
    sdkRoot: null,
    sdkmanager: null,
    avdmanager: null,
    emulator: null,
    avds: [],
    runningEmulators: [],
    recommendedAvd: "Locaryn_API34",
    detail: "Le SDK Android n'est pas joignable en mode navigateur. Lancez l'application Locaryn.",
  }),
  setupAndroidVm: async () => {
    throw new Error(
      "L'installation du SDK Android exige l'application Locaryn, pas le navigateur.",
    );
  },
  startAndroidVm: async () => {
    throw new Error("Démarrer un émulateur exige l'application Locaryn, pas le navigateur.");
  },
  stopAndroidVm: async () => {
    throw new Error("Arrêter un émulateur exige l'application Locaryn, pas le navigateur.");
  },
  androidScreenProbe: async () => {
    throw new Error("La capture écran Android exige l'application Locaryn, pas le navigateur.");
  },
  androidScreenAction: async () => {
    throw new Error("Le contrôle écran Android exige l'application Locaryn, pas le navigateur.");
  },
  writeTestAudio: async () => "demo://audio",
  removeTestAudio: async () => {},
  saveAudioAs: async (sourcePath, destinationPath) => {
    // Browser preview fallback: trigger a normal download when no native
    // filesystem command is available.
    const link = document.createElement("a");
    link.href = sourcePath;
    link.download = destinationPath.split(/[\\/]/).pop() || "note-vocale.wav";
    document.body.appendChild(link);
    link.click();
    link.remove();
  },
  saveImageAs: async (sourcePath, destinationPath) => {
    const link = document.createElement("a");
    link.href = sourcePath;
    link.download = destinationPath.split(/[\\/]/).pop() || "image.png";
    document.body.appendChild(link);
    link.click();
    link.remove();
  },
  approveToolCall: async (decision) => {
    // Demo mode: simulate the roundtrip so the UI's pending state clears.
    await sleep(150);
    if (typeof window !== "undefined") {
      // eslint-disable-next-line no-console
      console.info(`[demo approve] ${decision.decision} ${decision.tool} -> ${decision.scope}`);
    }
  },

  async updateProviderModelParams(_params) {
    // Demo: no-op, params not persisted in browser mode.
  },
  async getProviderModelParams() {
    return {
      temperature: 0.7,
      top_p: 0.95,
      top_k: 40,
      ctx_size: 8192,
      max_tokens: 0,
      repeat_penalty: 1.1,
      seed: -1,
    };
  },
  async searchOllamaLibrary() {
    return [];
  },
  async runtimeCapabilities() {
    return {
      runtime_installed: true,
      runtime_version: "b10088",
      chat: true,
      vision: false,
      embeddings: true,
      image_gen: true,
      finetune: true,
      distributed: true,
      speculative_decoding: true,
      kv_quant: true,
      weight_formats: ["GGUF"],
      unavailable: [
        "AWQ / EXL2 / GPTQ (nécessite ExLlamaV2 / vLLM)",
        "PagedAttention / vLLM (serveur GPU séparé)",
        "FlexGen : streaming SSD orchestré (mmap seul = lent)",
        "TurboQuant KV 3-bit (papier Google réel, mais non mergé dans llama.cpp)",
        "Entraînement LoRA/QLoRA, distillation, pruning (pile Python : Unsloth/PEFT)",
      ],
    };
  },
  async listLoraAdapters() {
    return [];
  },
  async setLoraAdapters(_scales) {},
  async ragIndexText(_projectId, source, text) {
    const n = Math.max(1, Math.ceil(text.length / 1000));
    return {
      chunk_count: n,
      dim: 2560,
      embed_model: "demo-model",
      sources: [{ source, chunks: n }],
    };
  },
  async ragStatus() {
    return { chunk_count: 0, dim: 0, embed_model: "", sources: [] };
  },
  async ragClear() {},
  async ragSearch(_projectId, query) {
    return [{ source: "demo.md", text: `(demo) extrait pertinent pour « ${query} »`, score: 0.82 }];
  },
  async listInferenceEngines() {
    return [] as InferenceEngineInfo[];
  },
  async startInferenceEngine(engine: string) {
    throw new Error(`Aucun socle local : impossible de démarrer « ${engine} » ici.`);
  },
  async stopInferenceEngine() {},
  async inferenceEngineLog() {
    return "";
  },
  async llamaRuntimeStatus() {
    return {
      installed: true,
      version: "b10088",
      up_to_date: true,
      pinned: "b10088",
      path: "C:/Users/you/.locaryn/data/bin/llama",
    };
  },
  async setupLlamaRuntime(_variant, onProgress) {
    for (const pct of [5, 25, 60, 90, 100]) {
      await sleep(250);
      onProgress?.(pct, pct < 100 ? "Téléchargement du runtime…" : "Runtime installé");
    }
    return {
      installed: true,
      version: "b10088",
      up_to_date: true,
      pinned: "b10088",
      path: "C:/Users/you/.locaryn/data/bin/llama",
    };
  },
  async modelResidency() {
    return demoResidencyStatus();
  },
  async checkModelFit(model) {
    return demoFit(model, demoCaution);
  },
  async cloudProviders() {
    return demoCloudProviders.map((p) => ({
      ...p,
      has_key: demoCloudState.key.length > 0,
      model_count: demoCloudModels.length,
      updated_at: new Date().toISOString(),
      active_model: demoCloudState.model,
    }));
  },
  async cloudProviderSetKey(_provider, key) {
    if (!key.trim()) throw new Error("La clé est vide.");
    demoCloudState.key = key.trim();
  },
  async cloudProviderClearKey() {
    demoCloudState.key = "";
    demoCloudState.model = null;
  },
  async cloudProviderModels() {
    // Un catalogue distant met un instant à répondre : sans ce délai, l'état
    // de chargement de l'écran ne se voit jamais.
    await new Promise((r) => setTimeout(r, 250));
    return demoCloudModels;
  },
  async cloudProviderStatus(provider) {
    const p = demoCloudProviders.find((x) => x.id === provider);
    const label = p?.label ?? provider;
    return {
      running: demoCloudState.running,
      installed: demoCloudState.installed,
      detail: demoCloudState.running
        ? `${label} répond sur ${p?.api_url ?? ""}.`
        : demoCloudState.installed
          ? `${label} est installée mais ne répond pas. Démarrez-la depuis ce dossier.`
          : `${label} n'est pas installée. Locaryn peut le faire : npm install -g omniroute.`,
      dashboard_url: p?.dashboard_url ?? null,
    };
  },
  async cloudProviderInstall(provider) {
    // Une installation par gestionnaire de paquets prend du temps : sans ce
    // délai, l'état « installation… » de l'écran ne se voit jamais.
    await new Promise((r) => setTimeout(r, 1200));
    demoCloudState.installed = true;
    const p = demoCloudProviders.find((x) => x.id === provider);
    return `${p?.label ?? provider} est installée.`;
  },
  async cloudProviderStart(provider) {
    // Démarrer installe d'abord quand le programme manque : c'est un
    // enchaînement qui n'a qu'une issue possible.
    if (!demoCloudState.installed) await demoCore.cloudProviderInstall(provider);
    await new Promise((r) => setTimeout(r, 900));
    demoCloudState.running = true;
    return demoCore.cloudProviderStatus(provider);
  },
  async cloudProviderOpenDashboard(provider) {
    const p = demoCloudProviders.find((x) => x.id === provider);
    return p?.dashboard_url ?? "";
  },
  async cloudProviderSelect(provider, model) {
    // Le refus sans clé est réel, pas décoratif : c'est le seul garde-fou qui
    // évite un appel payant sans authentification.
    if (!demoCloudState.key)
      throw new Error(
        `Aucune clé enregistrée pour ${provider}. Ouvrez son dossier dans « Mes modèles » et collez votre clé avant de choisir un modèle.`,
      );
    demoCloudState.model = model;
    // Le fournisseur actif bascule sur le distant, comme en vrai : sans cela,
    // le nom affiché sous le champ de saisie resterait celui du modèle local.
    const p = demoCloudProviders.find((x) => x.id === provider);
    demoProviders = [
      {
        id: `demo-cloud-${provider}`,
        kind: "remote",
        engine: "open_ai_compat",
        endpoint: p?.api_url ?? "",
        model,
        is_active: true,
        status: "healthy",
        config: { cloud_provider: provider },
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      },
    ];
    demoHealth.active_provider = {
      kind: "remote",
      engine: "open_ai_compat",
      endpoint: p?.api_url ?? "",
      model,
    };
  },
  async llmfitHardware() {
    return demoHardware;
  },
  async llmfitCatalog(entries) {
    return entries.map((entry) => demoFit(entry.id, demoCaution, entry.parameters_b));
  },
  async loadChatModel(model, force) {
    const fit = demoFit(model, demoCaution);
    // Le refus est réel, pas décoratif : sans lui, l'écran de garde-fou
    // serait impossible à voir autrement qu'en production.
    if (fit.verdict === "refuse" && !force) throw new Error(fit.message);
    // Un chargement instantané cacherait l'animation qu'on cherche à vérifier.
    await new Promise((r) => setTimeout(r, 1400));
    demoResident = { model, pinned: true, since: Date.now() };
    return demoResidencyStatus();
  },
  async ejectChatModel() {
    await new Promise((r) => setTimeout(r, 300));
    demoResident = { model: null, pinned: false, since: Date.now() };
    return demoResidencyStatus();
  },
  async cautionLevel() {
    return demoCaution;
  },
  async setCautionLevel(level) {
    demoCaution = level;
  },
  async checkHardware() {
    return {
      total_ram_gb: 32,
      total_vram_gb: 12,
      recommended_size_label: "mid (14-35B)",
    };
  },
  async getInferenceConfig(): Promise<InferenceConfig> {
    return {
      profile: "balanced",
      gpu_layers: -1,
      kv_cache_type: "q8_0",
      context_length: 8192,
      flash_attention: true,
      cpu_threads: 0,
      batch_size: 512,
      use_turboquant: false,
      draft_model_path: "",
      use_mmap: true,
      parallel_slots: 1,
      n_cpu_moe: 0,
      rpc_servers: "",
      lora_adapters: [],
    };
  },
  async setInferenceConfig(_config, consent) {
    if (
      consent === false &&
      _config.lora_adapters.some((p) =>
        /nsfw|uncensored|pony|urpm|realisticvision|abyssorangemix|counterfeit/i.test(p),
      )
    ) {
      throw new Error(
        "Un ou plusieurs adaptateurs LoRA sont classés NSFW / sans garde-fous. Acceptez la responsabilité avant de sauvegarder. [demo]",
      );
    }
  },
  async getProfilePreset(profile): Promise<InferenceConfig> {
    const presets: Record<string, InferenceConfig> = {
      eco: {
        profile: "eco",
        gpu_layers: 0,
        kv_cache_type: "f16",
        context_length: 4096,
        flash_attention: false,
        cpu_threads: 0,
        batch_size: 256,
        use_turboquant: false,
        draft_model_path: "",
        use_mmap: true,
        parallel_slots: 1,
        n_cpu_moe: 0,
        rpc_servers: "",
        lora_adapters: [],
      },
      balanced: {
        profile: "balanced",
        gpu_layers: -1,
        kv_cache_type: "q8_0",
        context_length: 8192,
        flash_attention: true,
        cpu_threads: 0,
        batch_size: 512,
        use_turboquant: false,
        draft_model_path: "",
        use_mmap: true,
        parallel_slots: 1,
        n_cpu_moe: 0,
        rpc_servers: "",
        lora_adapters: [],
      },
      performance: {
        profile: "performance",
        gpu_layers: -1,
        kv_cache_type: "q8_0",
        context_length: 16384,
        flash_attention: true,
        cpu_threads: 0,
        batch_size: 512,
        use_turboquant: false,
        draft_model_path: "",
        use_mmap: true,
        parallel_slots: 1,
        n_cpu_moe: 0,
        rpc_servers: "",
        lora_adapters: [],
      },
      turbo: {
        profile: "turbo",
        gpu_layers: -1,
        kv_cache_type: "q4_0",
        context_length: 32768,
        flash_attention: true,
        cpu_threads: 0,
        batch_size: 1024,
        use_turboquant: false,
        draft_model_path: "",
        use_mmap: true,
        parallel_slots: 1,
        n_cpu_moe: 0,
        rpc_servers: "",
        lora_adapters: [],
      },
      longctx: {
        profile: "longctx",
        gpu_layers: -1,
        kv_cache_type: "q4_0",
        context_length: 65536,
        flash_attention: true,
        cpu_threads: 0,
        batch_size: 1024,
        use_turboquant: false,
        draft_model_path: "",
        use_mmap: true,
        parallel_slots: 1,
        n_cpu_moe: 0,
        rpc_servers: "",
        lora_adapters: [],
      },
    };
    return presets[profile] ?? presets.balanced;
  },
  async openModelsFolder() {},
  // En démo (navigateur), un lien s'imite par ancre : #locaryn://install?src=…
  async pendingDeepLink() {
    const h = window.location.hash.replace(/^#/, "");
    return h.startsWith("locaryn://") ? h : null;
  },
};

// ============================================================================
// Export — pick the implementation based on the runtime
// ============================================================================

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** "tauri" = real core, "demo" = browser demo data. */
export const coreMode: "tauri" | "demo" = isTauri ? "tauri" : "demo";

export const core: CoreApi = isTauri ? tauriCore : demoCore;
