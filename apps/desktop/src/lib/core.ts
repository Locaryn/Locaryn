// Typed bridge to the in-process Rust core (Tauri commands + channels).
// Types mirror `locaryn-shared-types` and `locaryn-events` (serde snake_case).
//
// When the app runs outside Tauri (plain `vite dev` in a browser), a demo
// implementation with canned data takes over so the UI can be designed and
// tested without the Rust shell. The active mode is exposed as `coreMode`.

import { Channel, invoke } from "@tauri-apps/api/core";
import { resolveSeedGguf } from "./modelRegistry";

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

// ── SSH connector ──────────────────────────────────────────────────────────
export type SshAuthMethod = "password" | "key" | "agent";
export type SshAiAccess = "none" | "read_only" | "approval" | "trusted";
export type SshStatus = "unknown" | "ok" | "error";

export interface SshJump {
  host: string;
  port: number;
  username: string;
  auth_method: SshAuthMethod;
  key_path: string | null;
}

export interface SshServer {
  id: string;
  name: string;
  description: string;
  host: string;
  port: number;
  username: string;
  auth_method: SshAuthMethod;
  key_path: string | null;
  jump: SshJump | null;
  host_key_algo: string | null;
  host_key_sha256: string | null;
  host_key_verified: boolean;
  ai_access: SshAiAccess;
  capabilities: unknown;
  scope: string;
  status: SshStatus;
  enabled: boolean;
  last_connected_at: string | null;
  created_at: string;
  updated_at: string;
}

/** The form payload. `secret` is passed separately, never inside the draft. */
export interface SshServerDraft {
  name: string;
  description: string;
  host: string;
  port: number;
  username: string;
  auth_method: SshAuthMethod;
  key_path: string | null;
  scope?: string;
  jump: SshJump | null;
}

export interface SshServerPatch {
  name?: string;
  description?: string;
  host?: string;
  port?: number;
  username?: string;
  key_path?: string;
}

export interface SshProbeResult {
  reachable: boolean;
  os: string | null;
  whoami: string | null;
  can_read: boolean;
  can_write: boolean;
  is_sudoer: boolean;
  host_key: { algo: string; sha256: string };
  suggested_description: string;
  test_token: string;
}

export type SshTestEvent =
  | { type: "connecting" }
  | { type: "authenticating" }
  | { type: "probing"; step: string }
  | { type: "done" }
  | { type: "error"; message: string };

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
}

export interface ExtensionPermissionState {
  permission: ExtensionPermission;
  /** The plugin's own justification. Shown verbatim — it is not ours to edit. */
  reason: string | null;
  granted: boolean;
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
  permissions: ExtensionPermissionState[];
  /** Components that failed to parse. The plugin still runs without them. */
  load_errors: string[];
  created_at: string;
  updated_at: string;
}

/** How much of a catalog entry can actually run here. */
export type CatalogCompat = "native" | "adapted" | "partial" | "unsupported";

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
  | "secret";

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
  /** plugin.json, .claude-plugin/plugin.json, … */
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
  /** Most recently used project, if any. Nothing is created implicitly. */
  project: Project | null;
  /** Last open session in that project, if any. */
  session: Session | null;
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

/** Result of an image generation. `simulated` = placeholder, NOT a real render.
 *  `path` is the absolute disk path to the generated image; use `convertFileSrc`
 *  to turn it into a displayable URL. */
export interface GeneratedImage {
  /** The first (or only) image. */
  path: string;
  simulated: boolean;
  /** Every image produced, `path` included. Longer than one when several
   *  variants were requested in a single run. */
  variants?: string[];
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

// ── Region editing ─────────────────────────────────────────────────────
// Change one named part of an image and nothing else. The region is picked
// from a plain description, so this is not limited to clothing.

export interface RegionEditResult {
  path: string;
  /** Where the mask landed, so the UI can show what was selected. */
  mask_path: string;
  /** Share of the image covered by the mask, as a percentage. */
  coverage: number;
  /** Segmenter confidence, 0-1. */
  confidence: number;
  /** Connected pieces the selection breaks into. One or two means a real
   *  object; five scattered blobs means the description was too vague — a
   *  signal confidence alone misses, since tiling can score 0.88 on a
   *  selection spread across a wall, a door and a shelf. */
  pieces: number;
  /** Share of the selection held by its largest piece, 0-1. */
  largest: number;
}

export interface RegionEditArgs {
  /** Disk path or `data:` URL. */
  image: string;
  /** What to select, in plain words: "the t-shirt", "the wooden shelf". */
  target: string;
  /** "recolor" keeps the fabric and rewrites the colour exactly;
   *  "replace" redraws the region with the diffusion engine. */
  mode: "recolor" | "replace" | "preview";
  /** Target colour for "recolor", as #RRGGBB. */
  color?: string;
  /** What to draw instead, for "replace". */
  prompt?: string;
  model?: string;
  outputDir: string;
  steps?: number;
  cfgScale?: number;
  strength?: number;
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

/** Settings an administrator prepared for this machine. */
export interface Provisioning {
  serverUrl: string;
  organisation: string;
  certificateFingerprint: string | null;
  note: string;
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
  /** Prompt rewritten in English (diffusion models are trained on English). */
  english_prompt: string;
  /** "draft" | "standard" | "high" | "max" */
  quality: string;
  reason: string;
}

/** A plan produced by the model for a substantial request. */
export interface TaskPlan {
  needs_plan: boolean;
  /** Verify the result and replay the plan on failure (bug fixes). */
  needs_loop: boolean;
  steps: string[];
}

/** Defaults applied to every image generation unless explicitly overridden. */
export interface ImageDefaults {
  /** "draft" | "standard" | "high" | "max" | "custom" */
  quality: string;
  width: number;
  height: number;
  /** 0 = let the model family decide. */
  steps: number;
  cfg_scale: number;
  vram_mode: string;
  negative_prompt: string;
  /** How many variants to render per request, 1-8. */
  variants: number;
}

/** Named quality presets → pixels. Shared by the settings UI and slash args. */
export const IMAGE_QUALITIES: { id: string; label: string; px: number; hint: string }[] = [
  { id: "draft", label: "Brouillon", px: 256, hint: "Le plus rapide — icônes, essais" },
  { id: "standard", label: "Standard", px: 512, hint: "Bon compromis vitesse/qualité" },
  { id: "high", label: "Haute", px: 768, hint: "Plus détaillé, plus lent" },
  { id: "max", label: "Maximale", px: 1024, hint: "Qualité maximale, le plus lent" },
];

/** RAM/VRAM dispatch for image generation — lets big models run on any PC.
 *  gpu = all on GPU (fastest); auto = sd.cpp places modules by free memory;
 *  lowvram = weights kept in RAM, streamed into VRAM on demand. */
export type VramMode = "gpu" | "auto" | "lowvram";

/** State of the managed llama.cpp runtime. */
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
  vision: boolean;
  embeddings: boolean;
  image_gen: boolean;
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
  /** "nvidia" | "amd" | "intel" | "unknown" */
  gpu_vendor?: string;
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
  /** Taille des poids sur disque. */
  size_gb: number;
  /** Ce qu'il faut réellement, marge de prudence comprise. */
  required_gb: number;
  free_ram_gb: number;
  free_vram_gb: number;
  /** "gpu" | "ram" | "disque" | "inconnu" — où les poids finiront. */
  placement: string;
  level: CautionLevel;
  /** Peut-on forcer malgré le refus ? */
  overridable: boolean;
  /** Phrase montrée telle quelle : ce qui va se passer, pas un code. */
  message: string;
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
  /** Workspace directory for a session (project path, or temp folder for free
   *  chats). The temp folder is created only when `ensure` is true — a plain
   *  question must never leave a folder on disk. `exists` tells the caller
   *  whether the folder is actually there (so the UI stays empty when it is). */
  sessionWorkspace(sessionId: string, ensure?: boolean): Promise<{ path: string; exists: boolean }>;
  /** A plan the model produced for a substantial request. */
  planTask(request: string): Promise<TaskPlan>;
  /** Should this message be routed to the image generator? Prepares the
   *  English prompt; the user always confirms before anything is generated. */
  detectImageRequest(message: string): Promise<ImageIntent>;
  /** Background call: 1-click next-step suggestions after an answer. */
  suggestFollowups(answer: string): Promise<string[]>;
  /** Persist an assistant message (e.g. a generated image) into a session. */
  appendAssistantMessage(sessionId: string, content: string): Promise<void>;
  /** Saved image-generation defaults (quality, resolution, VRAM mode). */
  getImageDefaults(): Promise<ImageDefaults>;
  setImageDefaults(config: ImageDefaults): Promise<void>;
  /** How this machine should run a model (auto GPU / RAM-offload routing). */
  planModelRuntime(model: string): Promise<RuntimePlan>;
  listSessions(projectId: string): Promise<Session[]>;
  /** Create a chat; `title` auto-names it (e.g. from the first prompt). */
  createSession(projectId: string, title?: string): Promise<Session>;
  /** Rename a session. */
  updateSessionTitle(sessionId: string, title: string): Promise<void>;
  /** Ask the LLM to generate and persist a concise title for a session. */
  generateSessionTitle(sessionId: string, firstPrompt: string): Promise<string>;
  /** Permanently delete a session and its messages. */
  deleteSession(sessionId: string): Promise<void>;
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
  appInfo(): Promise<AppInfo>;

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

  /** Edit one named region, leaving everything else untouched. */
  editRegion(
    args: RegionEditArgs,
    onProgress?: (pct: number, detail?: string) => void,
  ): Promise<RegionEditResult>;

  serverStatus(): Promise<ServerStatus>;
  setServerMode(enabled: boolean, port?: number): Promise<ServerStatus>;
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
  listExtensions(): Promise<InstalledExtension[]>;
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

  /** MCP servers — shared with the daemon through `mcp.json`. */
  listMcpServers(): Promise<McpServerInfo[]>;
  addMcpServer(args: AddMcpServerArgs): Promise<McpServerInfo[]>;
  removeMcpServer(name: string): Promise<McpServerInfo[]>;
  /** Start a server and return the tools it announced. */
  startMcpServer(name: string): Promise<string[]>;
  stopMcpServer(name: string): Promise<void>;
  /** Invoke a tool through the same MCP client used by the agent runtime. */
  invokeMcpTool(name: string, tool: string, args: Record<string, unknown>): Promise<unknown>;

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
  listSshServers(): Promise<SshServer[]>;
  testSshConnection(
    draft: SshServerDraft,
    secret: string | null,
    onEvent: (ev: SshTestEvent) => void,
  ): Promise<SshProbeResult>;
  confirmSshHostKey(testToken: string): Promise<void>;
  saveSshServer(
    draft: SshServerDraft,
    secret: string | null,
    testToken: string,
  ): Promise<SshServer>;
  updateSshServer(id: string, patch: SshServerPatch): Promise<SshServer>;
  setSshAiAccess(id: string, level: SshAiAccess): Promise<SshServer>;
  deleteSshServer(id: string): Promise<void>;

  /** Send the user verdict+scope back to the runtime (doc 11 s6.5). */
  approveToolCall: (decision: ToolApprovalDecision) => Promise<void>;
  updateProviderModelParams(params: ModelParams): Promise<void>;
  getProviderModelParams(): Promise<ModelParams>;
  /** Install a model. `heretic` also auto-installs the uncensored companion
   *  weights (abliterated encoder) so the setup works with zero extra steps. */
  pullModel(
    endpoint: string,
    model: string,
    onProgress?: (pct: number, status?: string) => void,
    heretic?: boolean,
    consent?: boolean,
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
  /** Diffusion checkpoints only (aux files like VAE/text-encoder hidden). */
  listImageModels(): Promise<string[]>;
  /** True when an abliterated ("heretic") text encoder is installed, enabling
   *  the uncensored Z-Image variant in the image model picker. */
  hasAbliteratedEncoder(): Promise<boolean>;
  /** List local TTS/speech synthesis models. */
  listAudioModels(): Promise<string[]>;
  /** List available Kokoro voice names for a given Kokoro model. */
  listKokoroVoices(model: string): Promise<string[]>;
  /** Pick a voice reference audio file from disk. Returns null if cancelled. */
  pickVoiceReference(): Promise<string | null>;
  /** Generate music from a text prompt using a music generation model (Python-based).
   *  Returns the absolute path to the generated audio file. */
  /** Generate a 3D model from a text prompt or image using a local 3D
   *  generation model (Shape-E, Point-E, TripoSR, etc. — Python-based).
   *  Returns the absolute path to the generated 3D file (.obj/.glb/.ply). */
  generate3D(
    model: string,
    prompt: string,
    outputDir: string,
    inputImage?: string | null,
    negativePrompt?: string | null,
    steps?: number | null,
    cfgScale?: number | null,
    format?: string | null,
    onProgress?: (pct: number, detail?: string) => void,
  ): Promise<GeneratedAudio>;

  /** Generate music from a text prompt using a Python-based music generation
   *  model. Returns the absolute path to the generated audio file. */
  generateMusic(
    model: string,
    prompt: string,
    outputDir: string,
    duration?: number | null,
    melodyReference?: string | null,
    negativePrompt?: string | null,
    steps?: number | null,
    cfgScale?: number | null,
    onProgress?: (pct: number, detail?: string) => void,
  ): Promise<GeneratedAudio>;
  /** Generate a video from a text prompt or image using a local video
   *  generation model (Wan 2.1, LTX Video, SVD, etc. — Python-based).
   *  Returns the absolute path to the generated video file. */
  generateVideo(
    model: string,
    prompt: string,
    outputDir: string,
    duration?: number | null,
    inputImage?: string | null,
    negativePrompt?: string | null,
    steps?: number | null,
    cfgScale?: number | null,
    width?: number | null,
    height?: number | null,
    onProgress?: (pct: number, detail?: string) => void,
  ): Promise<GeneratedAudio>;
  /** Synthesize speech from text. Returns the absolute path to the generated
   *  audio file. */
  generateAudio(
    model: string,
    text: string,
    outputDir: string,
    voiceReference?: string,
    speaker?: string,
    speed?: number,
    pitch?: number,
    energy?: number,
    clarity?: number,
    language?: string,
    voiceDescription?: string,
    designPrompt?: string,
    sampling?: TtsSampling,
    onProgress?: (pct: number, detail?: string) => void,
  ): Promise<GeneratedAudio>;
  /** Status of the managed llama.cpp runtime (installed / up to date). */
  llamaRuntimeStatus(): Promise<LlamaRuntimeStatus>;
  /** Download + install the pinned llama.cpp runtime, streaming progress. */
  setupLlamaRuntime(
    variant?: "vulkan" | "cpu",
    onProgress?: (pct: number, status?: string) => void,
  ): Promise<LlamaRuntimeStatus>;
  generateImage(
    model: string,
    prompt: string,
    outputDir: string,
    inputImage?: string,
    negativePrompt?: string,
    steps?: number,
    cfgScale?: number,
    width?: number,
    height?: number,
    vramMode?: VramMode,
    uncensored?: boolean,
    consent?: boolean,
    /** Render this many variants in one run (1-8). One model load instead of
     *  several: measured 3 images in 140 s versus 181 s run separately. */
    variants?: number,
    onProgress?: (pct: number, detail?: string) => void,
  ): Promise<GeneratedImage>;
  checkHardware(): Promise<HardwareSpec>;

  /** Ce qui est actuellement en mémoire, et si le minuteur peut y toucher. */
  modelResidency(): Promise<ResidencyStatus>;
  /** Ce que donnerait le chargement de ce modèle, sans rien charger. */
  checkModelFit(model: string): Promise<ModelFit>;
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
  sessionWorkspace: (sessionId, ensure) =>
    invoke<{ path: string; exists: boolean }>("session_workspace", {
      sessionId,
      ensure: ensure ?? false,
    }),
  suggestFollowups: (answer) => invoke<string[]>("suggest_followups", { answer }),
  planTask: (request) => invoke<TaskPlan>("plan_task", { request }),
  detectImageRequest: (message) => invoke<ImageIntent>("detect_image_request", { message }),
  appendAssistantMessage: (sessionId, content) =>
    invoke<void>("append_assistant_message", { sessionId, content }),
  planModelRuntime: (model) => invoke<RuntimePlan>("plan_model_runtime", { model }),
  getImageDefaults: () => invoke<ImageDefaults>("get_image_defaults"),
  setImageDefaults: (config) => invoke<void>("set_image_defaults", { config }),
  listSessions: (projectId) => invoke<Session[]>("list_sessions", { projectId }),
  createSession: (projectId, title) =>
    invoke<Session>("create_session", { projectId, title: title ?? null }),
  updateSessionTitle: (sessionId, title) =>
    invoke<void>("update_session_title", { sessionId, title }),
  generateSessionTitle: (sessionId, firstPrompt) =>
    invoke<string>("generate_session_title", { sessionId, firstPrompt }),
  deleteSession: (sessionId) => invoke<void>("delete_session", { id: sessionId }),
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
  appInfo: () => invoke<AppInfo>("app_info"),

  listVoicePresets: () => invoke<VoicePreset[]>("list_voice_presets"),
  saveVoicePreset: (args) => invoke<VoicePreset>("save_voice_preset", { args }),
  deleteVoicePreset: (id) => invoke("delete_voice_preset", { id }),
  voicePresetSupport: (model) => invoke<EngineSupport>("voice_preset_support", { model }),

  editRegion: (args, onProgress) => {
    const chan = new Channel<{ progress: number; detail?: string }>();
    if (onProgress) chan.onmessage = (m) => onProgress(m.progress, m.detail);
    return invoke<RegionEditResult>("edit_region", { args, onProgress: chan });
  },

  serverStatus: () => invoke<ServerStatus>("server_status"),
  setServerMode: (enabled, port) =>
    invoke<ServerStatus>("set_server_mode", { args: { enabled, port: port ?? null } }),
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

  listExtensions: () => invoke<InstalledExtension[]>("list_extensions"),
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
  listSshServers: () => invoke<SshServer[]>("list_ssh_servers"),
  testSshConnection(draft, secret, onEvent) {
    const chan = new Channel<SshTestEvent>();
    chan.onmessage = onEvent;
    return invoke("test_ssh_connection", { draft, secret, onEvent: chan });
  },
  confirmSshHostKey: (testToken) => invoke("confirm_ssh_host_key", { testToken }),
  saveSshServer: (draft, secret, testToken) =>
    invoke<SshServer>("save_ssh_server", { draft, secret, testToken }),
  updateSshServer: (id, patch) => invoke<SshServer>("update_ssh_server", { id, patch }),
  setSshAiAccess: (id, level) => invoke<SshServer>("set_ssh_ai_access", { id, level }),
  deleteSshServer: (id) => invoke("delete_ssh_server", { id }),
  approveToolCall: (decision) => invoke("approve_tool_call", { payload: decision }),

  updateProviderModelParams: (params) => invoke("update_provider_model_params", { params }),
  getProviderModelParams: () => invoke<ModelParams>("get_provider_model_params"),
  pullModel: (endpoint, model, onProgress, heretic, consent) => {
    // Tags du catalogue style Ollama (gemma2:2b…) → URL GGUF directe. Le
    // backend local ne connaît pas Ollama et rejette les tags sans URL.
    const modelUrl = resolveSeedGguf(model) ?? model;
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
      model: modelUrl,
      heretic: heretic ?? null,
      consent: consent ?? null,
      hfToken: hfToken || null,
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
  listImageModels: () => invoke<string[]>("list_image_models"),
  hasAbliteratedEncoder: () => invoke<boolean>("has_abliterated_encoder"),
  listAudioModels: () => invoke<string[]>("list_audio_models").catch(() => []),
  listKokoroVoices: (model) => invoke<string[]>("list_kokoro_voices", { model }).catch(() => []),
  pickVoiceReference: () => invoke<string | null>("pick_voice_reference"),
  generateMusic: (
    model,
    prompt,
    outputDir,
    duration,
    melodyReference,
    negativePrompt,
    steps,
    cfgScale,
    onProgress,
  ) => {
    const chan = new Channel<{ progress: number; detail?: string }>();
    if (onProgress) chan.onmessage = (m) => onProgress(m.progress, m.detail);
    return invoke<GeneratedAudio>("generate_music", {
      model,
      prompt,
      outputDir,
      duration: duration ?? null,
      melodyReference: melodyReference ?? null,
      negativePrompt: negativePrompt ?? null,
      steps: steps ?? null,
      cfgScale: cfgScale ?? null,
      onProgress: chan,
    });
  },
  generateVideo: (
    model,
    prompt,
    outputDir,
    duration,
    inputImage,
    negativePrompt,
    steps,
    cfgScale,
    width,
    height,
    onProgress,
  ) => {
    const chan = new Channel<{ progress: number; detail?: string }>();
    if (onProgress) chan.onmessage = (m) => onProgress(m.progress, m.detail);
    return invoke<GeneratedAudio>("generate_video", {
      model,
      prompt,
      outputDir,
      duration: duration ?? null,
      inputImage: inputImage ?? null,
      negativePrompt: negativePrompt ?? null,
      steps: steps ?? null,
      cfgScale: cfgScale ?? null,
      width: width ?? null,
      height: height ?? null,
      onProgress: chan,
    });
  },
  generate3D: (
    model,
    prompt,
    outputDir,
    inputImage,
    negativePrompt,
    steps,
    cfgScale,
    format,
    onProgress,
  ) => {
    const chan = new Channel<{ progress: number; detail?: string }>();
    if (onProgress) chan.onmessage = (m) => onProgress(m.progress, m.detail);
    return invoke<GeneratedAudio>("generate_3d", {
      model,
      prompt,
      outputDir,
      inputImage: inputImage ?? null,
      negativePrompt: negativePrompt ?? null,
      steps: steps ?? null,
      cfgScale: cfgScale ?? null,
      format: format ?? null,
      onProgress: chan,
    });
  },
  generateAudio: (
    model,
    text,
    outputDir,
    voiceReference,
    speaker,
    speed,
    pitch,
    energy,
    clarity,
    language,
    voiceDescription,
    designPrompt,
    sampling,
    onProgress,
  ) => {
    const chan = new Channel<{ progress: number; detail?: string }>();
    if (onProgress) chan.onmessage = (m) => onProgress(m.progress, m.detail);
    return invoke<GeneratedAudio>("generate_audio", {
      model,
      text,
      outputDir,
      voiceReference: voiceReference ?? null,
      speaker: speaker ?? null,
      speed: speed ?? null,
      pitch: pitch ?? null,
      energy: energy ?? null,
      clarity: clarity ?? null,
      language: language ?? null,
      voiceDescription: voiceDescription ?? null,
      designPrompt: designPrompt ?? null,
      // Rust reads these through serde with camelCase renaming, so the shape
      // crosses the boundary as-is.
      sampling: sampling ?? null,
      onProgress: chan,
    });
  },
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
  generateImage: (
    model,
    prompt,
    outputDir,
    inputImage,
    negativePrompt,
    steps,
    cfgScale,
    width,
    height,
    vramMode,
    uncensored,
    consent,
    variants,
    onProgress,
  ) => {
    const chan = new Channel<{ progress: number; detail?: string }>();
    if (onProgress) chan.onmessage = (m) => onProgress(m.progress, m.detail);
    return invoke<GeneratedImage>("generate_image", {
      model,
      prompt,
      outputDir,
      inputImage: inputImage ?? null,
      negativePrompt: negativePrompt ?? null,
      steps: steps ?? null,
      cfgScale: cfgScale ?? null,
      width: width ?? null,
      height: height ?? null,
      vramMode: vramMode ?? null,
      uncensored: uncensored ?? null,
      consent: consent ?? null,
      variants: variants ?? null,
      onProgress: chan,
    });
  },
  checkHardware: () => invoke("check_hardware"),

  modelResidency: () => invoke<ResidencyStatus>("model_residency"),
  checkModelFit: (model) => invoke<ModelFit>("check_model_fit", { model }),
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
    type_id: "ssh",
    display_name: "SSH Remote Server",
    summary:
      "Connexion serveur distant via SSH — exécution de commandes et administration assistée par l'IA.",
    icon: "🖧",
    category: "connector",
    source: "built-in",
    available: true,
    supports_test: true,
    install_hint: "",
  },
  {
    type_id: "mcp_custom",
    display_name: "Serveur MCP Personnalisé",
    summary:
      "Ajoutez n'importe quel serveur Model Context Protocol (STDIO ou SSE) via sa commande ou son URL HTTP.",
    icon: "🛠️",
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
    icon: "🐙",
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
    icon: "🦊",
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
    icon: "🔍",
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
    icon: "🐘",
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
    icon: "🐳",
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
    icon: "⚡",
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
    icon: "🐍",
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
    icon: "🎭",
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
    icon: "📋",
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
    icon: "🧠",
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
    icon: "☁️",
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
    icon: "💬",
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
        "📁",
        "Lecture/écriture de fichiers avec contrôle d'accès configurable.",
        "filesystem",
      ],
      ["mcp_git", "Git", "🔀", "Lire, chercher et manipuler des dépôts Git.", "git"],
      [
        "mcp_fetch",
        "Fetch",
        "🌐",
        "Récupère une page web et la convertit pour le modèle.",
        "fetch",
      ],
      [
        "mcp_memory",
        "Memory",
        "🧠",
        "Mémoire persistante sous forme de graphe de connaissances.",
        "memory",
      ],
      [
        "mcp_sequential",
        "Sequential Thinking",
        "💭",
        "Résolution de problèmes par séquences de raisonnement.",
        "sequential-thinking",
      ],
      ["mcp_time", "Time", "🕒", "Date, heure et conversions de fuseaux horaires.", "time"],
      [
        "mcp_everything",
        "Everything (démo)",
        "🧪",
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
    icon: "🦁",
    category: "extension",
    source: "MCP communauté",
    available: true,
    supports_test: false,
    install_hint: "npx -y @brave/brave-search-mcp-server",
  },
];

// Nothing is installed by default — the user adds connectors from Browse.
let demoSshServers: SshServer[] = [];

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
let demoExtensions: InstalledExtension[] = [
  {
    id: "demo-1",
    name: "code-review",
    display_name: "code-review",
    version: "1.0.0",
    api_version: "0.1",
    description: "Relecture automatique des diffs par agents spécialisés.",
    author: "Anthropic",
    homepage: "https://github.com/anthropics/claude-code",
    kind: "plugin",
    scope: "user",
    ecosystem: "claude_code",
    source: "github:anthropics/claude-code#plugins/code-review",
    install_dir: "~/.locaryn/plugins/code-review",
    enabled: true,
    components: {
      skills: 2,
      commands: 3,
      agents: 4,
      rules: 0,
      hooks: 0,
      mcp_servers: 0,
      lsp_adapters: 0,
    },
    permissions: [{ permission: "files_read", reason: "Lire le diff à relire", granted: true }],
    load_errors: [],
    created_at: "2026-07-20T10:00:00Z",
    updated_at: "2026-07-20T10:00:00Z",
  },
  {
    id: "demo-2",
    name: "security",
    display_name: "security",
    version: "0.5.0",
    api_version: "0.1",
    description: "Audit de dépendances et scan OSV.",
    author: "gemini-cli-extensions",
    homepage: "https://github.com/gemini-cli-extensions/security",
    kind: "plugin",
    scope: "user",
    ecosystem: "gemini_cli",
    source: "https://github.com/gemini-cli-extensions/security",
    install_dir: "~/.locaryn/plugins/security",
    enabled: false,
    components: {
      skills: 1,
      commands: 2,
      agents: 0,
      rules: 1,
      hooks: 0,
      mcp_servers: 1,
      lsp_adapters: 0,
    },
    permissions: [
      { permission: "mcp", reason: "Lancer le serveur d'analyse OSV", granted: false },
      { permission: "network", reason: "Interroger la base de vulnérabilités", granted: false },
    ],
    load_errors: [],
    created_at: "2026-07-22T09:00:00Z",
    updated_at: "2026-07-22T09:00:00Z",
  },
];

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
    advertised: ["mcp", "commands", "★412"],
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

/** Taille déduite du nom du fichier, faute de disque à mesurer. */
function demoModelSizeGb(model: string): number {
  const m = /(\d+(?:[.,]\d+)?)\s*b\b/i.exec(model);
  const billions = m ? Number.parseFloat(m[1].replace(",", ".")) : 7;
  // ~0,6 Go par milliard de paramètres en quantification 4 bits.
  return Math.max(0.3, billions * 0.6);
}

function demoFit(model: string, level: CautionLevel): ModelFit {
  const size = demoModelSizeGb(model);
  const [factor, reserve] =
    level === "prudent" ? [1.35, 3.0] : level === "equilibre" ? [1.12, 1.5] : [1.0, 0.0];
  const required = size * factor + reserve;
  const fitsVram = demoMemory.freeVramGb > 0 && required <= demoMemory.freeVramGb;
  const fitsRam = required <= demoMemory.freeRamGb;

  if (fitsVram)
    return {
      model,
      verdict: "confortable",
      size_gb: size,
      required_gb: required,
      free_ram_gb: demoMemory.freeRamGb,
      free_vram_gb: demoMemory.freeVramGb,
      placement: "gpu",
      level,
      overridable: false,
      message: `${size.toFixed(1)} Go sur le GPU, ${demoMemory.freeVramGb.toFixed(1)} Go libres. Vitesse maximale.`,
    };
  if (fitsRam)
    return {
      model,
      verdict: "juste",
      size_gb: size,
      required_gb: required,
      free_ram_gb: demoMemory.freeRamGb,
      free_vram_gb: demoMemory.freeVramGb,
      placement: "ram",
      level,
      overridable: false,
      message: `${size.toFixed(1)} Go à répartir : trop pour les ${demoMemory.freeVramGb.toFixed(1)} Go de VRAM libres, le reste ira en RAM. Plus lent qu'en tout-GPU.`,
    };
  if (level === "risque")
    return {
      model,
      verdict: "risque",
      size_gb: size,
      required_gb: required,
      free_ram_gb: demoMemory.freeRamGb,
      free_vram_gb: demoMemory.freeVramGb,
      placement: "disque",
      level,
      overridable: false,
      message: `${size.toFixed(1)} Go demandés pour ${demoMemory.freeRamGb.toFixed(1)} Go libres. Le système va compenser sur le disque : ralentissement sévère, et l'application peut être tuée par manque de mémoire.`,
    };
  return {
    model,
    verdict: "refuse",
    size_gb: size,
    required_gb: required,
    free_ram_gb: demoMemory.freeRamGb,
    free_vram_gb: demoMemory.freeVramGb,
    placement: "disque",
    level,
    overridable: true,
    message: `${size.toFixed(1)} Go demandés, ${required.toFixed(1)} Go nécessaires avec la marge choisie, et seulement ${demoMemory.freeRamGb.toFixed(1)} Go libres. Fermez des applications, choisissez un modèle plus petit, ou passez le niveau de prudence sur « risqué » pour forcer.`,
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
  sessionWorkspace: async (_sessionId, ensure) => {
    if (ensure) return { path: "/tmp/locaryn-demo", exists: true };
    return { path: "/tmp/locaryn-demo", exists: false };
  },
  appendAssistantMessage: async () => {},
  detectImageRequest: async (message) => {
    const m =
      /\b(image|photo|dessin|logo|ic[oô]ne|illustration|visuel|g[ée]n[èe]re[rz]?|dessine)\b/i.test(
        message,
      );
    return {
      is_image: m,
      is_edit: /\b(modifie|[ée]dite|retouche|change)\b/i.test(message),
      english_prompt: m ? `a high quality picture: ${message}` : "",
      quality: /ic[oô]ne|rapide|brouillon/i.test(message) ? "draft" : "standard",
      reason: m ? "La demande décrit un visuel à produire." : "",
    };
  },
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
  createSession: async (projectId, title) => {
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
  pullModel: async (_endpoint, model, onProgress, _heretic, consent) => {
    const modelUrl = resolveSeedGguf(model) ?? model;
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
      onProgress?.(i, `Téléchargement de ${modelUrl}... ${i}%`);
    }
    if (!demoModels.includes(modelUrl)) {
      demoModels.push(modelUrl);
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

  editRegion: async (args, onProgress) => {
    onProgress?.(10, "analyse de l'image");
    await sleep(300);
    onProgress?.(100, "termine");
    return {
      path: `${args.outputDir}/edit_demo.png`,
      mask_path: "",
      coverage: 18.3,
      confidence: 0.97,
      pieces: 1,
      largest: 1,
    };
  },

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

  listExtensions: async () => demoExtensions,
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
      },
      permissions: [
        { permission: "mcp", reason: "Lancer son serveur MCP", granted: false },
        { permission: "files_read", reason: "Lire le projet", granted: false },
      ],
      load_errors: [],
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
    manifest_file: "plugin.json",
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
  listSshServers: async () => demoSshServers,
  async testSshConnection(draft, _secret, onEvent) {
    onEvent({ type: "connecting" });
    await sleep(450);
    if (!draft.host.trim()) {
      onEvent({ type: "error", message: "host is required" });
      throw new Error("host is required");
    }
    onEvent({ type: "authenticating" });
    await sleep(450);
    onEvent({ type: "probing", step: "capabilities" });
    await sleep(550);
    onEvent({ type: "done" });
    const user = draft.username || "deploy";
    return {
      reachable: true,
      os: "Linux demo 6.5.0-x86_64",
      whoami: user,
      can_read: true,
      can_write: draft.auth_method !== "agent",
      is_sudoer: false,
      host_key: {
        algo: "ssh-ed25519",
        sha256: `SHA256:demo${Math.random().toString(36).slice(2, 18)}`,
      },
      suggested_description: `Linux demo host reachable as ${user}@${draft.host}:${draft.port}. Read: yes. Write ($HOME): ${draft.auth_method !== "agent" ? "yes" : "no"}. Sudo: no.`,
      test_token: "demo-token",
    };
  },
  confirmSshHostKey: async () => {},
  async saveSshServer(draft, _secret, _testToken) {
    const s: SshServer = {
      id: `demo-ssh-${Math.random().toString(36).slice(2, 7)}`,
      name: draft.name,
      description: draft.description || "saved in demo mode",
      host: draft.host,
      port: draft.port,
      username: draft.username,
      auth_method: draft.auth_method,
      key_path: draft.key_path,
      jump: draft.jump,
      host_key_algo: "ssh-ed25519",
      host_key_sha256: "SHA256:demoSaved",
      host_key_verified: true,
      ai_access: "none",
      capabilities: null,
      scope: draft.scope ?? "user",
      status: "ok",
      enabled: true,
      last_connected_at: null,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };
    demoSshServers = [...demoSshServers, s];
    return s;
  },
  async updateSshServer(id, patch) {
    demoSshServers = demoSshServers.map((s) => (s.id === id ? { ...s, ...patch } : s));
    return demoSshServers.find((s) => s.id === id) ?? demoSshServers[0];
  },
  async setSshAiAccess(id, level) {
    demoSshServers = demoSshServers.map((s) => (s.id === id ? { ...s, ai_access: level } : s));
    return demoSshServers.find((s) => s.id === id) ?? demoSshServers[0];
  },
  async deleteSshServer(id) {
    demoSshServers = demoSshServers.filter((s) => s.id !== id);
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
  async listImageModels() {
    return ["z_image_turbo-Q8_0.gguf", "sd_xl_turbo_1.0.q8_0.gguf"];
  },
  async hasAbliteratedEncoder() {
    return true;
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
  async generateImage(
    _model,
    _prompt,
    _outputDir,
    _in,
    _neg,
    steps,
    _cfg,
    _w,
    _h,
    _vram,
    _unc,
    consent,
    _variants,
    onProgress,
  ) {
    // Demo: simulate a step-by-step generation so the progress bar is testable.
    const total = steps ?? 8;
    for (let i = 1; i <= total; i++) {
      await sleep(400);
      onProgress?.(Math.round((i / total) * 100), `étape ${i}/${total}`);
    }
    return {
      // Demo mode has no real file, so we keep an inline data URL in the path
      // field. The consumer treats data: URLs as already-displayable.
      path: `data:image/svg+xml;base64,${btoa(
        '<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512"><rect width="512" height="512" fill="#1e2022"/><text x="50%" y="50%" fill="#6f9c7f" font-size="28" text-anchor="middle">demo</text></svg>',
      )}`,
      simulated: true,
    };
  },
  async listAudioModels() {
    return ["piper-voices/en_US-amy-medium.onnx", "coqui-xtts-v2"];
  },
  async listKokoroVoices() {
    return ["af_heart", "am_fenrir", "bf_siwis", "ff_siwis"];
  },
  async pickVoiceReference() {
    // Demo mode: no real file picker available in the browser.
    return null;
  },
  async generateMusic(
    _model,
    prompt,
    _outputDir,
    _duration,
    _melodyRef,
    _negPrompt,
    _steps,
    _cfgScale,
    onProgress,
  ) {
    const total = 5;
    for (let i = 1; i <= total; i++) {
      await sleep(400);
      onProgress?.(Math.round((i / total) * 100), `génération musicale ${i}/${total}`);
    }
    return {
      path: "data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQAAAAA=",
      simulated: true,
    };
  },
  async generateVideo(
    _model,
    _prompt,
    _outputDir,
    _duration,
    _inputImage,
    _negPrompt,
    _steps,
    _cfgScale,
    _width,
    _height,
    onProgress,
  ) {
    const total = 5;
    for (let i = 1; i <= total; i++) {
      await sleep(400);
      onProgress?.(Math.round((i / total) * 100), `génération vidéo ${i}/${total}`);
    }
    return {
      path: "",
      simulated: true,
    };
  },
  async generate3D(
    _model,
    _prompt,
    _outputDir,
    _inputImage,
    _negPrompt,
    _steps,
    _cfgScale,
    _format,
    onProgress,
  ) {
    const total = 5;
    for (let i = 1; i <= total; i++) {
      await sleep(400);
      onProgress?.(Math.round((i / total) * 100), `génération 3D ${i}/${total}`);
    }
    return {
      path: "",
      simulated: true,
    };
  },
  async generateAudio(
    _model,
    _text,
    _outputDir,
    _voiceReference,
    _speaker,
    _speed,
    _pitch,
    _energy,
    _clarity,
    _language,
    _voiceDescription,
    _designPrompt,
    _sampling,
    onProgress,
  ) {
    const total = 5;
    for (let i = 1; i <= total; i++) {
      await sleep(300);
      onProgress?.(Math.round((i / total) * 100), `étape audio ${i}/${total}`);
    }
    return {
      path: "data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQAAAAA=",
      simulated: true,
    };
  },
  async modelResidency() {
    return demoResidencyStatus();
  },
  async checkModelFit(model) {
    return demoFit(model, demoCaution);
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
