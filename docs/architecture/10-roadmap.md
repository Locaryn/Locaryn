# 10 — Roadmap Lochor

> Document maître de planification. Couvre l'intégralité du prompt produit original.
> Dernière mise à jour : session de bootstrap + intégration Liquid Glass + thème dynamique.

---

## 0. Inventaire de l'existant (post-bootstrap)

Le monorepo est bootstrappé et compile vert (`cargo check`, `cargo clippy -D warnings`, `cargo test`, `tsc --noEmit`). Voici l'état réel de chaque composant.

### Légende
- **Squelette** : types/structures définis, compile, mais logique métier absente (stub/mock/placeholder).
- **Partiel** : une partie de la logique est implémentée, le reste est stub.
- **Fonctionnel** : implémentation réelle utilisable.

### Packages Rust (le cœur partagé)

| Crate | État | Ce qui existe | Ce qui manque |
|-------|------|---------------|---------------|
| `shared-types` | **Fonctionnel** | Tous les types (Project, Session, Message, Task, Artifact, Provider, Health, ApiError, Permission, TrustLevel, ConnectionMode, ToolCall). | Rien (types stables). |
| `config` | **Fonctionnel** | Chargement TOML par scope (global/user/workspace), `global_dir()`, remote config, TLS config, parsing serde. | Validation stricte, schéma JSON, migration de config. |
| `events` | **Fonctionnel** | `StreamEvent` (Token, ToolCall, ToolResult, Artifact, PreviewUpdate, ProviderChanged, MessageEnd, Error, TaskUpdate), encodage/décodage SSE, `sse_stream()`. | Rien (utilisable tel quel). |
| `storage` | **Squelette** | `Database` struct, `open()` + `open_memory()` + `sqlx::migrate!`, 6 repos (Project, Session, Message, Task, Artifact, Provider) avec interfaces typées. | **Toutes les requêtes SQL réelles** (les repos retournent des stubs vides/hardcodés). |
| `auth` | **Réel** | Argon2id salé, jetons issus du CSPRNG système, vérification en temps constant, trait `Keychain` + `SystemKeychain`. Autorité mTLS et émission de certificats dans `config::mtls`. | Rotation automatique des jetons, keychain OS pour le stockage client. |
| `sdk` | **Fonctionnel** | `LochorClient` HTTP/SSE complet : health, info, projects CRUD, sessions CRUD, messages (send_message → stream), tasks (cancel, approve), providers (list, switch, start_local), `resolve_auto()` fallback remote→local. | Gestion retry, timeout configurable, reconnect SSE. |
| `preview` | **Partiel** | `PreviewRequest`, `PreviewRender`, `PreviewOrigin` (Tauri/Daemon), `wrap_html()` (CSP strict/network), `resolve_render()`, `artifact_to_request()`. | `render_markdown()` réel (marked + sanitize), export Python → PNG/HTML, serving depuis le daemon. |
| `extensions` | **Partiel** | `PluginManifest` + validation, `Components` (hooks/skills/commands/slashCommands/agents/mcpServers/rules/lsp), `PermissionRequest` (reason/scope/requireApproval), `Registry` (install_from_dir, reload, list, import_claude_code, import_cursor), `ExtensionScope` (Global/User/Workspace). | **DB-backed registry** (in-memory), **hot-reload via fs watcher**, résolution dépendances, sandbox d'exécution, marketplace local. |
| `mcp` | **Squelette** | `McpConfig` (`.mcp.json` parsing), `McpServerEntry` (transport stdio/http), `Transport` enum, `McpClient` trait, `StubClient`, `build_client()`, `config_path()` par scope. | **Client MCP réel via `rmcp`**, spawn subprocess stdio, HTTP stateless, tool discovery, tool invocation. |
| `plugin-sdk` | **Partiel** | `PluginBuilder` (fluent API), `ToolDecl`, `LochorPlugin` trait (async_trait), `PluginError`, re-exports manifest types. | **WASM sandbox** (wasmtime), proc macro `#[lochor_plugin]`, runtime d'exécution. |
| `command-runtime` | **Squelette** | `CommandDef` (name/description/prompt), `CommandRegistry` (register/list/resolve), parsing frontmatter TOML. | Exécution réelle (dispatch vers agent avec prompt injecté), variables de template, chaînes de commandes. |
| `hook-runtime` | **Partiel** | `HookEvent` (9 events Claude-Code-compatibles), `HooksFile` parsing (PascalCase + snake_case), `MatcherEntry`, `HookAction`, `run_hook()` (spawn cmd/bash avec timeout), `hook_env()`. | Exécution async réelle, parallel hooks, hook chain ordering, veto/block logic, stdout parsing pour décisions. |
| `skill-runtime` | **Partiel** | `SkillBundle` (frontmatter name/description/version/auto_trigger/allowed_tools), `SkillRegistry` (register/list/auto_trigger_candidates), parsing TOML frontmatter. | Matching sémantique (embeddings), priorité de skills conflictuels, injection contextuelle. |
| `agent-runtime` | **Squelette** | `Agent` trait, `AgentInput`, `ToolContext`, `AgentProfile` (frontmatter TOML), `AgentRegistry`, `EventStream`, `StubAgent` (echo), `tools.rs` (tool set V1 : read_file/write_file/run_command/search/list_dir). | **Boucle agent réelle** (tool dispatch, approval gating, streaming, subagents, provider calls Ollama/OpenAI-compat). |
| `rules-runtime` | **Partiel** | `RuleFile`, `RuleScope` (Global/Workspace), `load_all()` (global + workspace, frontmatter `priority:` override), `system_prompt_fragment()` (composition markdown). | Validation de règles, conflit resolution, rules hot-reload. |
| `lsp-adapters` | **Squelette** | Interface LSP déclarée (adaptateurs rust-analyzer, typescript-language-server). | **Client LSP réel** (tower-lsp ou manuel over stdio), exposure comme tools agent (go-to-def, diagnostics, hover). |

### Services

| Service | État | Ce qui existe | Ce qui manque |
|---------|------|---------------|---------------|
| `daemon` | **Partiel** | axum serveur loopback :7474, routes : `/health`, `/v1/projects` (GET/POST), `/v1/sessions/:id` (GET), `/v1/sessions/:id/messages` (POST → SSE via StubAgent), `/v1/providers` (GET), `DaemonState` (mode, start_time). | **Storage wiring** (repos retournent stubs), **agent réel** (StubAgent → OllamaAgent), provider supervisor integration, extensions/MCP/hooks/skills/commands endpoints, audit, file serving pour preview. |
| `remote-server` | **Squelette** | axum serveur, routes : `/health`, `/v1/auth/login` (stub token), `/v1/*` (401 sans token). Args : bind addr, TLS cert/key. | **TLS (rustls)**, **auth réel** (Argon2id + token issuance + verify), sessions/permissions, providers côté serveur, audit logs, rate limiting, streaming, extensions côté serveur. |
| `provider-supervisor` | **Partiel** | CLI clap (status/health/start), `parse_engine()`, `default_endpoint()` (Ollama/LlamaCpp/Lmstudio/Vllm), `healthcheck()` (HTTP GET /v1/models + fallback /api/version Ollama), `print_status()`. | **Spawn réel** (tokio::process), idle shutdown, healthcheck loop, integration avec daemon, auto-start Ollama. |

### Apps

| App | État | Ce qui existe | Ce qui manque |
|-----|------|---------------|---------------|
| `cli` | **Partiel** | clap CLI complet : `status`, `chat` (interactive via SDK), `projects add/list`, `sessions new/list`, `providers list/use/health/start`, `plugins list`, `mcp list/start/discover`, `daemon start/stop/logs`, `import claude-code/cursor`. | **Toutes les commandes sont stubs** (println, pas de vraies opérations sauf chat qui parle au daemon via SDK). |
| `desktop` | **Partiel** | Tauri v2 + React/TS, 4 panneaux, Liquid Glass CSS + thème dynamique, Sen font. **S5 fait** : cœur in-process (même SQLite que daemon/CLI), commands `bootstrap`/`send_message`/`run_terminal` + CRUD projets/sessions/messages, streaming `ipc::Channel`, ChatPanel réel (tokens, tool cards, historique persisté), LeftPanel réel (sélection projet/session, création), terminal line-based, TopBar branché sur `core_health`. | **Terminal PTY** (xterm.js + resize/couleurs), **Monaco éditeur**, **preview réel** (iframe + artifact loading), approval UI interactive, extensions/settings UI, provider switching UI. |

### UI packages

| Package | État | Notes |
|---------|------|-------|
| `packages-ui/core` | **Partiel** | Tokens (fontFamily, fontFamilyMono, colors), Button, Panel. Non wired au thème dynamique. |
| `packages-ui/chat` | **Partiel** | ChatPanel component (messages + input). Inline styles, non wired au thème. |
| `packages-ui/preview` | **Partiel** | PreviewFrame (iframe sandboxed), PreviewTabs. Inline styles. |
| `packages-ui/terminal` | **Squelette** | Terminal component (pre + lines). xterm.js pas encore intégré. |

### Infrastructure

| Élément | État | Notes |
|---------|------|-------|
| SQLite migrations | **Fonctionnel** | 0001_init.sql (11 tables), 0002_extensions.sql (10 tables). Toutes les tables du prompt sont définies. |
| CI (GitHub Actions) | **Squelette** | Workflow files existent, besoin de vérifier le contenu. |
| Liquid Glass + thème | **Fonctionnel** | global.css complet (mesh gradient animé, glass panels, accent dynamique), useTheme hook (localStorage + CSS vars), SettingsPanel (swatches, sliders, toggles). |
| Sen font | **Fonctionnel** | 4 fichiers woff2 (400/600/700/800) bundlés localement + OFL license. |
| Icons (32×32 PNG/ICO) | **Fonctionnel** | Placeholder généré via scripts/gen-icons.js. |

---

## 0 bis. Fait hors calendrier initial (août 2026)

Travaux réalisés en dehors du découpage S1–S10, documentés dans
[12-server-and-deployment.md](12-server-and-deployment.md).

| Sujet | État | Note |
| --- | --- | --- |
| Authentification réelle | OK | Argon2id + CSPRNG. Les primitives précédentes étaient des factices étiquetés `argon2id$placeholder$` |
| Comptes et jetons | OK | `lochor users`, révocation immédiate, pas d'énumération possible |
| TLS du daemon | OK | Certificat fourni ou auto-signé généré une fois ; empreinte SHA-256 affichée au démarrage |
| Sécurité liée à l'adresse d'écoute | OK | Exposé ⇒ authentification **et** TLS obligatoires ; refuse de démarrer sans compte |
| Déploiement pré-configuré | OK | `lochor provision` produit `lochor-connect.json` |
| CLI en deux modes | OK | `lochor` = agent avec outils ; `lochor chat` = conversation seule |
| Raisonnement replié | OK | Desktop et CLI ; découpage partagé dans `agent-runtime` |
| Chaîne de publication | OK | `.msi`, `.deb`, portables, `.apk` — release en brouillon |
| Mode serveur dans l'interface | OK | Paramètres → Partage réseau ; l'app supervise le daemon plutôt que de servir elle-même |
| Lecture du fichier de déploiement par l'app | à faire | |
| mTLS (certificats clients) | OK | Optionnel ; autorité locale, serveur signé par elle |
| Ouverture de port UPnP | OK | Refusée si mTLS inactif ; bail d'une heure renouvelé |
| Écran de connexion (fichier de déploiement) | OK | Certificat installable depuis l'app ; empreinte du serveur vérifiée |
| Mode voyage (tunnel sortant + QR signé) | OK | cloudflare / ngrok / devtunnel |
| Application Android | OK | Tauri v2 ; appairage par QR, jetons graphiques partagés ; APK non signé (clé = décision du client) |
| Client mobile | à faire | |
| Répartition de charge | à faire | Invariant : uniquement sur les machines du demandeur |

Corrections de fond trouvées en chemin et consignées ici parce qu'elles
touchaient des chemins critiques :

- Un modèle TTS ou d'image accepté comme modèle de conversation faisait échouer
  llama-server, et le daemon répondait alors **silencieusement** avec son agent
  factice. Garde-fou posé à l'écriture du fournisseur.
- Un projet archivé bloquait son chemin définitivement : l'index UNIQUE couvre
  aussi les lignes effacées en douceur. Il est désormais restauré.
- Les tables `users` et `auth_tokens` existaient depuis 0001 **sans colonne de
  mot de passe** ; `username` était unique mais sensible à la casse.

## 1. MVP — 9 à 10 semaines (cœur viable)

### Objectif

Un utilisateur solo peut installer Lochor, ouvrir un projet, chatter avec un agent (via Ollama local), lire/éditer des fichiers, exécuter des commandes (avec approval), générer et prévisualiser un artefact HTML, et reprendre la même session en CLI.

### Work breakdown par semaine

| Semaine | Livrables | Crates touchés | Critères d'acceptation |
|---------|-----------|----------------|----------------------|
| **S1** ✅ | Monorepo bootstrappé, tous crates compilent, CI gate vert, Liquid Glass + thème dynamique | Tous | `cargo check` + `clippy -D warnings` + `cargo test` + `tsc --noEmit` verts. |
| **S2** ✅ | Storage réel : implémenter les 6 repos avec vraies requêtes SQL (INSERT/SELECT/UPDATE). Wiring daemon → storage. | `storage`, `daemon` | `lochor projects add` crée une vraie ligne en DB. `lochor sessions list` retourne les vraies sessions. |
| **S3** ✅ | Provider Ollama réel : implémenter `OllamaAgent` qui appelle `http://localhost:11434/api/chat` avec streaming. Brancher `StubAgent` → `OllamaAgent` dans le daemon. Streaming tokens SSE daemon → CLI. | `agent-runtime`, `events`, `daemon`, `cli` | `lochor chat` produit des tokens réels via Ollama local. Premier token < 1s. |
| **S4** ✅ | Boucle tool-use + approval : implémenter la boucle agent avec dispatch des tools (`read_file`, `write_file`, `run_command`, `search`, `list_dir`). Approval gating (run_command et write_file demandent consentement). Provider-supervisor réel : spawn `ollama serve` via tokio::process, healthcheck loop, auto-start. | `agent-runtime`, `provider-supervisor`, `daemon` | Tool approval fonctionne (run_command demande approval). `lochor provider start ollama` lance Ollama. Auto-start si Ollama absent. |
| **S5** ✅ | Desktop agent wiring : cœur in-process (même SQLite que daemon/CLI), Tauri commands `bootstrap`/`send_message`/CRUD projets-sessions-messages, streaming via `tauri::ipc::Channel`, ChatPanel réel (tokens + tool cards + historique), LeftPanel réel (projets/sessions), terminal line-based via `run_terminal` (PTY xterm.js reporté en V1). | `desktop` (Tauri + React), `agent-runtime` | Desktop chat produit des tokens réels. Terminal exécute des commandes. Desktop et CLI partagent la même session SQLite (test : créer session en CLI, la voir dans le desktop). |
| **S6** | Preview panel réel : iframe sandboxed + CSP, `event: artifact` → render HTML/markdown. `lochor-preview` `render_markdown()` réel (marked + sanitize). Monaco mini pour code blocks. File serving depuis le daemon (`/preview/:id`). | `preview`, `desktop`, `daemon` | Preview d'un artefact HTML sandboxed fonctionne. Markdown rendu correctement. Code blocks éditables dans Monaco. |
| **S7** | Extensions + MCP + plugin-sdk : install plugin via `plugin.json`, registry DB-backed, permissions prompt, `.lochor/mcp.json` loading, 1 MCP server stdio de démo. Hot-reload via fs watcher. | `extensions`, `mcp`, `plugin-sdk`, `daemon` | 1 plugin installable via `lochor plugin install ./examples/plugins/my-plugin`. 1 MCP server stdio chargeable via `.lochor/mcp.json`. Hot-reload détecte un changement de `plugin.json`. |
| **S8** | Commands + skills + hooks : slash commands exécution réelle (dispatch vers agent avec prompt injecté), skills auto-trigger (matching par mots-clés), hooks (PreToolUse/PostToolUse/Stop) exécution async avec timeout. | `command-runtime`, `skill-runtime`, `hook-runtime`, `agent-runtime` | `/refactor` slash command s'exécute. Skill auto-trigger suggère un skill quand l'utilisateur demande une migration DB. Hook PreToolUse bloque un run_command non approuvé. |
| **S9** | Rules + subagents + import : `LOCHOR.md` + rules agrégées dans system prompt (hot-reload), agent profiles + subagents (spawn agent spécialisé), `lochor import claude-code` et `lochor import cursor` (conversion réelle des bundles). | `rules-runtime`, `agent-runtime`, `extensions` | Rules `LOCHOR.md` apparaissent dans le system prompt. Subagent spécialisé s'exécute pour une tâche de refactor. `lochor import claude-code ./examples` importe un bundle (plugin.json + hooks + skills convertis). |
| **Buffer** | Polish, tests unitaires + intégration, packaging (MSI/DMG/AppImage), CI build matrix, release `v0.1.0`. | Tous | Tests verts. Packaging génère des binaires pour Win11 x64, macOS arm64, Linux x64. |

### Définition of Done MVP

- [ ] `cargo check --workspace` + `cargo clippy -D warnings` + `cargo test` verts.
- [ ] `pnpm typecheck` + `pnpm lint` verts.
- [ ] Desktop launch sur Win11 x64, macOS arm64, Linux x64.
- [ ] CLI `lochor chat` produit des tokens via Ollama local.
- [ ] Desktop et CLI partagent la même session (test : créer session en CLI, la voir dans le desktop).
- [ ] Tool approval fonctionne (run_command demande approval).
- [ ] Preview d'un artefact HTML sandboxed fonctionne.
- [ ] 1 plugin installable via `lochor plugin install ./examples/plugins/my-plugin`.
- [ ] 1 MCP server stdio chargeable via `.lochor/mcp.json`.
- [ ] `/refactor` slash command s'exécute.
- [ ] `lochor import claude-code ./examples` importe un bundle.
- [ ] Settings panel : accent color, glass blur, mesh toggle fonctionnent et persistent.

---

## 2. V1 — 3 mois (produit complet open-source)

### 2.1 Remote-server sécurisé

| Item | Description | Crates |
|------|-------------|--------|
| TLS (rustls) | Terminate TLS dans le remote-server via `axum-server` + `rustls`. Support auto-cert (Let's Encrypt) via reverse proxy (Caddy/Traefik) en V1, natif en V1.1. | `remote-server` |
| Auth réel | Argon2id hash des passwords, token issuance (JWT ou opaque), token verification middleware, rotation de tokens. | `auth`, `remote-server` |
| Sessions/permissions | Sessions utilisateur persistées, permissions par projet, RBAC minimal (admin/user). | `remote-server`, `storage` |
| Providers côté serveur | Configuration des providers distants côté serveur (OpenAI-compatible, vLLM, etc.). Le serveur agit comme gateway sécurisée. | `remote-server`, `agent-runtime` |
| Audit logs | `audit_logs` table remplie pour chaque action sensible (login, tool execution, provider switch, extension install). | `storage`, `remote-server` |
| Rate limiting | Rate limiting minimal par IP et par token (tower middleware). | `remote-server` |
| Streaming | SSE streaming des réponses agent à travers le remote-server (proxy transparent). | `remote-server`, `events` |
| Healthchecks | `/health` + `/v1/providers/:id/health` pour vérifier les providers côté serveur. | `remote-server` |
| Extensions côté serveur | Inventaire et activation des extensions côté serveur lorsque permis par les permissions. | `remote-server`, `extensions` |
| Packaging | Binaire natif + systemd service + conteneur Docker. Compatible Windows et Linux. | `remote-server` |

### 2.2 Mode auto + fallback

| Item | Description |
|------|-------------|
| Healthcheck remote | `resolve_auto()` healthcheck le serveur distant (timeout 30s), fallback local si indisponible. |
| Bascule propre | Transition sans perte de contexte : la session en cours continue sur le provider local. |
| Banner signalétique | UI affiche clairement le mode actif (remote/local/auto) et le provider utilisé. |
| Détection d'indisponibilité | Timeout + retry + circuit breaker pour éviter les blocages. |
| Démarrage runtime local | Si le local n'est pas prêt, proposer ou déclencher le démarrage du provider-supervisor. |

### 2.3 LSP adapters

| Item | Description | Crates |
|------|-------------|--------|
| rust-analyzer | Client LSP over stdio, exposé comme tools agent (go-to-def, diagnostics, hover, rename). | `lsp-adapters`, `agent-runtime` |
| typescript-language-server | Idem pour TypeScript/JavaScript. | `lsp-adapters` |
| python-lsp-server | Idem pour Python (pylsp). | `lsp-adapters` |
| Configuration | `.lochor/lsp.json` pour configurer les serveurs LSP par projet. | `lsp-adapters`, `config` |

### 2.4 Packaging complet

| Target | Format | Arch |
|--------|--------|------|
| Windows | MSI + NSIS | x64 + ARM64 |
| macOS | DMG | Apple Silicon + x64 |
| Linux | AppImage + deb | x64 + ARM64 |
| Remote-server | Binaire + Docker + systemd | x64 + ARM64 |

CI release : tags → build matrix → GitHub Releases + SHA256 + cosign (signing).

### 2.5 Documentation

- Guide utilisateur (desktop + CLI).
- Guide extension author (plugin.json, hooks, skills, commands, agents, MCP, rules, LSP).
- Guide déploiement remote-server (TLS, auth, systemd, Docker, reverse proxy).
- Guide contribution (architecture, conventions, CI).

---

## 3. V1.1 — +2 mois (sécurité + enterprise)

| Item | Description | Crates |
|------|-------------|--------|
| **mTLS** | Certificat client pour le remote-server. Support Tailscale/Headscale pour déploiements homelab/entreprise. | `auth`, `remote-server` |
| **Rate limiting avancé** | Par utilisateur, par endpoint, par extension. Sliding window. | `remote-server` |
| **IP allowlist** | Liste blanche d'IPs côté remote-server. | `remote-server` |
| **WASM plugins** | `wasmtime` sandbox pour exécuter du code plugin natif en sécurité. Proc macro `#[lochor_plugin]`. | `plugin-sdk`, `extensions` |
| **Marketplace local** | Index d'extensions installables depuis une URL ou un repo git. `lochor plugin install <url>`. | `extensions` |
| **Python viz** | Export graphiques Python → plotly HTML / matplotlib PNG dans le panel preview. | `preview` |
| **Audit log UI** | Visualisation des audit logs dans le desktop (admin). Filtres par action, utilisateur, date. | `desktop`, `storage` |
| **Token rotation** | Rotation automatique des tokens (30 jours), révocation, liste de tokens actifs. | `auth` |
| **Keychain OS réel** | Intégration `keyring` crate (Windows Credential Manager, macOS Keychain, Linux Secret Service). | `auth` |
| **Focus management** | Focus trap + focus return dans le SettingsPanel et toutes les modales desktop. | `desktop` |

---

## 4. V2 — +6 mois (enterprise + scale)

| Item | Description | Crates |
|------|-------------|--------|
| **Enterprise module complet** | Contexte partagé pré-indexé pour gros projets (vector index + chunk store). Orchestration DGX Spark. RBAC complet. SSO OIDC/SAML. | `remote-server/enterprise` |
| **PostgreSQL optionnel** | Abstraction `lochor-storage` pour supporter PostgreSQL en plus de SQLite (remote-server uniquement). | `storage` |
| **Marketplace distant** | Catalogue d'extensions signées (cosign), notation, reviews, install en un clic. | `extensions` |
| **Realtime collaboration** | Partage de session live entre utilisateurs (CRDT ou OT). Presence cursors. | `events`, `remote-server` |
| **Mobile** | Tauri v2 mobile — iOS/Android client léger (chat + preview, pas d'éditeur). | `apps/desktop` (mobile) |
| **Multilingue UI** | i18n du desktop (français, anglais, espagnol, allemand minimum). | `desktop` |
| **Subagents avancés** | Orchestration de sous-agents spécialisés en parallèle, fusion des résultats. | `agent-runtime` |
| **Context canary** | Détection de dégradation de contexte (perte de fil, hallucination) avec recovery protocol. | `agent-runtime` |

---

## 5. Sécurité — traversal (toutes phases)

La sécurité est une exigence transverse qui progresse à chaque phase.

| Exigence | MVP | V1 | V1.1 | V2 |
|----------|-----|----|----|-----|
| **Sandbox preview** | iframe `sandbox="allow-scripts"` + CSP strict (fait dans `lochor-preview`) | Idem + CSP network variant | Idem | Idem |
| **Permissions explicites** | `PermissionRequest` (reason/scope/requireApproval) dans plugin manifest (fait) | UI de consentement dans desktop | Sandbox WASM | RBAC |
| **Approval gating** | Tool approval (run_command, write_file demandent approval) | Idem + scopes (once/session/project/always) | Idem | Idem |
| **Séparation workspace/runtime** | Workspace utilisateur vs runtime interne séparés | Idem | Idem | Idem |
| **Politique de confiance** | `TrustLevel` (Trusted/Untrusted/Sandbox) par projet (fait dans shared-types) | Idem | Idem | Idem |
| **Loopback only** | Provider-supervisor écoute sur 127.0.0.1 uniquement (fait) | Idem | Idem | Idem |
| **Gateway sécurisée** | N/A (MVP = local only) | Remote-server comme gateway vers providers | mTLS | RBAC + SSO |
| **API key / token** | Token généré (squelette) | Argon2id + token réel | Rotation + mTLS | SSO OIDC/SAML |
| **TLS** | N/A (MVP = loopback) | rustls dans remote-server | mTLS | Idem + auto-cert |
| **Rate limiting** | N/A | Minimal (IP + token) | Avancé (par user/endpoint) | Idem |
| **Audit logs** | Table définie (fait) | Remplissage réel | UI de visualisation | Compliance exports |
| **Rotation credentials** | N/A | Token 30 jours | Auto-rotation | Idem + SSO |

---

## 6. Extensibilité — traversal (toutes phases)

| Élément | MVP | V1 | V1.1 | V2 |
|---------|-----|----|----|-----|
| **MCP servers** | `.mcp.json` loading + 1 server stdio démo (squelette → réel) | Client MCP `rmcp` complet (stdio + HTTP) | Idem | Marketplace distant |
| **Plugins** | `plugin.json` manifest + registry + install (partiel → réel) | DB-backed registry + permissions UI | WASM sandbox + marketplace local | Marketplace distant |
| **Slash commands** | Exécution réelle (S8) | Variables de template | Idem | Idem |
| **Hooks** | PreToolUse/PostToolUse/Stop exécution (S8) | Hook chain + veto logic | Idem | Idem |
| **Skills** | Auto-trigger (S8) | Matching sémantique (embeddings) | Idem | Idem |
| **Agents spécialisés** | Agent profiles + subagents (S8) | Subagents parallèles | Idem | Orchestration avancée |
| **Workspace rules** | `LOCHOR.md` + rules agrégées (S8) | Hot-reload + conflit resolution | Idem | Idem |
| **LSP adapters** | Squelette | rust-analyzer + tsserver + pylsp | Idem | Plus de langages |
| **Scopes** | Global/User/Workspace (fait dans `ExtensionScope`) | + Organisation scope (RBAC) | + Session scope | Idem |
| **Hot-reload** | fs watcher (S7) | Idem | Idem | Idem |
| **Import Claude Code** | `import_claude_code` (partiel → réel S8) | Import Cursor + Continue.dev | Idem | Idem |
| **Compatibilité MCP** | Standard MCP (squelette) | Spec complète via `rmcp` | Idem | Idem |

### Compatibilité avec écosystème Claude Code / Antigravity

| Concept | Compatible MCP standard ? | Nécessite adaptateur ? | Spécifique Lochor ? |
|---------|--------------------------|----------------------|---------------------|
| MCP servers (stdio) | **Oui** (direct) | Non | Non |
| MCP servers (HTTP stateless) | **Oui** (direct, spec 2026-07-28) | Non | Non |
| Hooks (PreToolUse etc.) | Non (concept Claude Code) | **Adaptateur** : `import_claude_code` convertit `hooks.json` → Lochor hooks | Lochor vocabulary (compatible) |
| Skills (markdown bundles) | Non | **Adaptateur** : conversion frontmatter | Format Lochor (compatible) |
| Slash commands | Non (concept Claude Code) | **Adaptateur** : conversion | Format Lochor (compatible) |
| Agent profiles | Non | **Adaptateur** : conversion | Format Lochor (compatible) |
| Workspace rules (CLAUDE.md) | Non | **Adaptateur** : `CLAUDE.md` → `LOCHOR.md` | Lochor (compatible) |
| LSP adapters | Non (concept Lochor) | N/A | **Spécifique Lochor** |
| Plugin manifest | Non (concept Lochor) | N/A | **Spécifique Lochor** (inspiré de Claude Code) |
| Permissions model | Non (concept Lochor) | N/A | **Spécifique Lochor** |

---

## 7. Persistence — schéma SQLite

Les migrations sont déjà définies (`migrations/0001_init.sql` + `migrations/0002_extensions.sql`).

### Tables MVP (déjà créées, à peupler)

| Table | Contenu | Local uniquement ? | Synchro serveur ? |
|-------|---------|-------------------|-------------------|
| `projects` | Projets/workspaces | Local + serveur | Synchro possible |
| `sessions` | Sessions de chat | Local + serveur | Synchro possible |
| `messages` | Messages (user/assistant/tool) | Local + serveur | Synchro possible |
| `tasks` | Tâches (tool calls, status) | Local + serveur | Synchro possible |
| `artifacts` | Artefacts (HTML, MD, PNG, Python) | Local + serveur | Synchro possible |
| `providers` | Configuration providers | Local + serveur | Synchro possible |
| `runtime_state` | État du runtime (daemon, provider actif) | **Local uniquement** | Non |
| `users` | Utilisateurs (remote-server) | **Serveur uniquement** | N/A |
| `auth_tokens` | Tokens d'authentification | **Serveur uniquement** | N/A |
| `audit_logs` | Logs d'audit | **Serveur uniquement** (local en V1.1) | Non (sensible) |

### Tables extensions (déjà créées, à peupler)

| Table | Contenu | Scope |
|-------|---------|-------|
| `extensions` | Métadonnées plugins | Global/user/workspace |
| `extension_installs` | Installations par scope | Global/user/workspace |
| `extension_permissions` | Permissions accordées/refusées | Par install |
| `mcp_servers` | Serveurs MCP enregistrés | Global/user/workspace |
| `commands` | Commandes personnalisées | Par plugin |
| `slash_commands` | Slash commands | Par plugin |
| `hooks` | Hooks enregistrés | Par plugin |
| `skills` | Skills bundles | Par plugin |
| `agent_profiles` | Profils d'agents spécialisés | Par plugin |
| `workspace_rules` | Règles de workspace | Par projet |
| `lsp_adapters` | Adaptateurs LSP | Par projet |

### Données sensibles (jamais exposées par défaut)

- **API keys / tokens** : stockés dans le keychain OS, jamais en clair en DB.
- **Variables d'environnement** : jamais loggées.
- **Contenu de fichiers hors workspace** : jamais lu sans permission explicite.
- **Audit logs** : jamais synchronisés vers le client (serveur uniquement).

---

## 8. UX — desktop et CLI

### Desktop (Liquid Glass)

| Élément | MVP | V1 |
|---------|-----|-----|
| Panneau gauche (sessions/projets) | Liste des projets + sessions, arbre collapsible | Recherche, filtres, drag-drop |
| Panneau central (chat/éditeur) | Chat avec streaming, composer avec slash commands | Monaco inline pour code blocks, diff view |
| Panneau bas (terminal/logs) | xterm.js + PTY via Tauri command | Tabs (terminal/logs/extensions/provider), logs filtrables |
| Panneau droit (preview/artefacts) | iframe sandboxed HTML, markdown rendu | Python PNG/plotly, multi-artefact tabs |
| TopBar | Logo, projet, provider badge, settings ⚙ | Mode switcher (remote/local/auto), provider selector |
| Settings panel | Accent color, glass blur, tint, mesh toggle, speed | Provider config, extensions management, permissions, audit logs |
| États loading/error/offline | Skeleton loaders, error toasts, offline banner | Idem + retry automatique |
| Signalétique remote/local | Provider badge color + texte | Banner + indicateur de latence |
| Extensions UI | N/A (MVP = CLI only) | Install/activate/désactivate, permissions prompt, MCP servers list |
| Rules UI | N/A | Éditeur `LOCHOR.md`, preview du system prompt |

### CLI

| Commande | MVP | V1 |
|----------|-----|-----|
| `lochor status` | Mode, provider, daemon, projets | + extensions actives, MCP servers, health |
| `lochor chat` | Streaming via daemon, slash commands | + subagents, skills auto-trigger |
| `lochor projects add/list` | CRUD via daemon | + import, export |
| `lochor sessions new/list` | Via daemon | + reprise, fork |
| `lochor providers list/use/health/start` | Via daemon + supervisor | + remote providers |
| `lochor plugins list/install/remove` | Registry | + marketplace, permissions |
| `lochor mcp list/start/discover` | MCP registry | + tool discovery, tool call |
| `lochor daemon start/stop/logs` | Process management | + config, TLS |
| `lochor import claude-code/cursor` | Bundle import | + continue.dev, cline |

---

## 9. Priorisation résumée

| Priorité | Items |
|----------|-------|
| **P0 (MVP)** | Desktop+CLI même cœur, daemon local, Ollama, tool-use, preview HTML, plugins/MCP/skills/commands/hooks de base, import Claude Code, Liquid Glass + thème |
| **P1 (V1)** | Remote-server sécurisé (TLS + auth + audit + streaming), fallback auto, packaging x64+ARM64, LSP, documentation |
| **P2 (V1.1)** | mTLS, WASM plugins, Python viz, marketplace local, token rotation, keychain OS, focus management |
| **P3 (V2)** | Enterprise collab/DGX, PostgreSQL, SSO, marketplace distant, mobile, multilingue, subagents avancés |

---

## 10. Risques et mitigations

| Risque | Impact | Probabilité | Mitigation |
|--------|--------|------------|------------|
| `rmcp` maturité insuffisante | MCP non fonctionnel | Moyenne | Wrapper d'abstraction dans `lochor-mcp` ; fallback client MCP HTTP stateless manuel ; tests d'intégration avec serveurs MCP de référence. |
| Tauri webview différences pour preview | Preview cassé sur certaines plateformes | Moyenne | Standardiser artefacts HTML portables ; tests cross-webview CI ; fallback `about:blank` + injection JS. |
| Sidecar provider-supervisor cross-arch | Ollama ne démarre pas sur ARM64 | Faible | Build matrix CI ; fallback "user-provided Ollama" si binaire absent ; documentation d'installation manuelle. |
| BSL adoption enterprise | Adoption limitée | Faible | Change date 4 ans + gate fonctionnel (pas juridique) + core 100% Apache ; BSL uniquement sur enterprise module. |
| Courbe Rust équipe | Vélocité réduite | Moyenne | Crates squelettes clairs ; beaucoup d'exemples ; CI stricte dès le départ ; pairing sessions. |
| Performance streaming | UX laggy | Faible | SSE + channels Tauri ; benchmark premier token < 1s ; backpressure handling. |
| `backdrop-filter` performance Tauri | Liquid Glass laggy sur GPU faible | Faible | Fallback `prefers-reduced-transparency` (déjà implémenté) ; slider glass blur (déjà implémenté) ; test sur GPU Intel UHD. |
| Storage SQLite concurrency | Locks sous charge | Faible | WAL mode (déjà activé) ; pool de connexions ; migration PostgreSQL en V2 si besoin. |

---

## 11. Conventions de nommage (rappel)

| Élément | Convention | Exemple |
|---------|-----------|---------|
| Binaires | `lochor-*` | `lochor` (CLI), `lochor-daemon`, `lochor-remote-server`, `lochor-supervisor` |
| Crates | `lochor-*` | `lochor-shared-types`, `lochor-agent-runtime` |
| Packages UI | `@lochor/ui-*` | `@lochor/ui-core`, `@lochor/ui-chat` |
| Extension scopes | `Global` / `User` / `Organisation` (V1) / `Workspace` / `Session` (V1.1) | `.lochor/` (workspace), `~/.lochor/` (user), `LOCHOR.md` (project) |
| Config files | `.lochor/` | `.lochor/mcp.json`, `.lochor/config.toml`, `LOCHOR.md` |
| Plugin manifest | `plugin.json` | `examples/plugins/my-plugin/plugin.json` |
| MCP config | `mcp.json` | `.lochor/mcp.json` |
| Hooks | `hooks.json` | `.lochor/hooks.json` |
| Skills | `*.md` (frontmatter) | `.lochor/skills/db-migration.md` |
| Agent profiles | `*.md` (frontmatter) | `.lochor/agents/refactorer.md` |
| Workspace rules | `*.md` | `.lochor/rules/security.md`, `LOCHOR.md` |
| Env vars | `LOCHOR_*` | `LOCHOR_TLS_CERT`, `LOCHOR_DAEMON_PORT` |

---

## 12. Versioning et release

| Phase | Version | CalVer |
|-------|---------|--------|
| Pre-MVP | `0.0.x` | Itérations rapides |
| MVP | `0.1.0` | Première release taggée |
| V1 | `1.0.0` | Stable |
| V1.1 | `1.1.0` | Sécurité + enterprise |
| V2 | `2.0.0` | Scale + collab |

Release strategy : tags Git → CI build matrix → GitHub Releases (binaires + SHA256 + cosign) + changelog auto-généré.

---

## 13. Livrables de cette session (déjà accomplis)

1. **Monorepo bootstrappé** : 16 crates Rust, 3 services, 2 apps, 4 packages-ui, 10 docs d'architecture, 5 ADRs, examples, migrations, CI.
2. **CI gate vert** : `cargo check` + `cargo clippy -D warnings` + `cargo test` + `tsc --noEmit` = 0 errors, 0 warnings.
3. **Police Sen** : 4 fichiers woff2 (400/600/700/800) bundlés localement + licence OFL.
4. **Direction artistique Liquid Glass** : mesh gradient animé, verre dépoli sur tous les panneaux, highlights spéculaires, fallbacks `prefers-reduced-transparency`/`motion`.
5. **Thème dynamique** : `useTheme` hook (localStorage + CSS vars), `SettingsPanel` (8 presets verts + color picker, sliders blur/tint/speed, mesh toggle, reset), Escape key, aria-labels, `role="dialog"`.
6. **Accent vert Jade Pulse** (`#2BFF88`) : naturel mais flashy, 7 autres presets disponibles.
