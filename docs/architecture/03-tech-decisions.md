# 03 — Tech Decisions

Architecture finale: **A** (cf. `02-architecture.md`). Décisions explicites ci-dessous.

## D1 — Desktop: Tauri v2 (vs Electron)

**Décision: Tauri v2 + React/TS.**

| Critère | Tauri v2 | Electron |
| --- | --- | --- |
| Taille binaire | ~10–20MB (shell) | 150–200MB |
| RAM | webview système | Chromium full |
| Partage core Rust | **natif** (in-process) | FFI ou subprocess |
| Sidecar (bundled binary) | ✅ `bundle.externalBin` + `tauri-plugin-shell` | ✅ child process |
| IPC | commands / events / **channels** (v2) | IPC Node main↔renderer |
| Build x64+ARM64 Win/macOS/Linux | ✅ `tauri-action` | ✅ mais lourd |
| Preview HTML sandboxed | ✅ iframe + CSP (Edge WebView2 / WebKit) | ✅ Chromium full |
| Mobile (futur) | ✅ first-class V2 | ❌ |

**Justification:** le brief exige un desktop natif léger + partage du cœur Rust. Tauri v2 est le seul à offrir un embarquement in-process du core Rust sans FFI. La preview d'artefacts HTML/CSS/JS standard fonctionne en webview système (Edge WebView2 sur Win11, WebKit sur macOS/Linux); les artefacts exotiques nécessitant Chromium-strict sont un non-objectif V1.

**Risque accepté:** différences webview Edge vs WebKit pour artefacts très avancés. Mitigation: preview standardisée sur HTML/CSS/JS portable + tests cross-webview en CI.

## D2 — Daemon: Rust (vs Python/Go)

**Décision: Rust.**

- **Partage natif avec Tauri** (le desktop embarque `locaryn-*` crates en in-process).
- Single binary, distribution simple, cross-arch (x64+ARM64) via `cargo`.
- MCP Rust SDK (`rmcp`) first-class en 2026.
- `tokio` pour streaming tokens, `tokio::process` pour supervisor.
- Memory safety critique pour le permission-gating des commandes shell.

L'argument décisif: **daemon, remote-server, desktop et CLI partagent les mêmes crates Rust.** Python ou Go imposerait un FFI ou une duplication — contraire au brief.

## D3 — Remote-server: Rust (vs Go/Python)

**Décision: Rust**, même crates que le daemon en mode "server".

- Partage de `locaryn-auth`, `locaryn-storage`, `locaryn-extensions`, `locaryn-mcp`, etc.
- `axum` + `rustls` pour TLS, `tower` pour rate limiting/middleware.
- Module enterprise (`services/remote-server/enterprise/`) en BSL 1.1: collaboration, DGX Spark orchestration, gate clients concurrents.
- Binaire unique déployable en service système / conteneur.

Go aurait été excellent pour un service réseau isolé, mais **dupliquerait la logique agentique** — non acceptable.

## D4 — Transport: HTTP/SSE (vs gRPC/WebSocket)

**Décision: HTTP/1.1 + SSE pour l'API locale et distante.**

| Critère | HTTP/SSE | gRPC | WebSocket |
| --- | --- | --- | --- |
| Standard, debuggable (curl, navigateur) | ✅ | requires grpc tooling | moyen |
| Streaming server→client | ✅ SSE | ✅ streams | ✅ |
| Streaming bidirectionnel | bidir via 2 SSE ou POST+SSE | ✅ natif | ✅ natif |
| Browser natif (EventSource) | ✅ | ❌ (grpc-web) | ✅ |
| Même contrat local + remote | ✅ | ✅ | ✅ |
| Frontend Tauri (fetch/EventSource) | ✅ trivial | besoin grpc-web | OK |

**Justification:** SSE couvre 95% des besoins (tokens, logs, état tâche, état preview — tous server→client). Pour les rares flux bidirectionnels (preview live reload, interactive shell), on utilise **POST + SSE response** ou un canal Tauri côté desktop. gRPC ajoute une dépendance protoc + grpc-web pour un gain marginal. L'API reste REST-ish + SSE, debuggable au curl.

**Exception:** le transport **MCP** suit la spec MCP 2026-07-28 (stateless HTTP + stdio), géré par `locaryn-mcp` indépendamment de notre API.

## D5 — Stockage: SQLite (+ filesystem workspace)

**Décision: SQLite (sqlx) pour métadonnées/sessions/historique/extensions; filesystem structuré pour workspaces et artefacts.**

- Local-first, zero-config, embarqué dans le daemon et le client.
- Migrations versionnées (`migrations/`).
- Schéma détaillé en `07-persistence.md`.
- **Données serveur-side** (remote-server enterprise): SQLite par défaut, schema compatible PostgreSQL pour montée en charge V2 (abstraction via `locaryn-storage`).

## D6 — Preview: iframe sandboxed + CSP strict

**Décision:**
- HTML/CSS/JS: iframe `sandbox="allow-scripts"` (pas `allow-same-origin`) + CSP strict (pas de réseau sauf permission `network` accordée). Servi depuis une origine dédiée (`locaryn-preview://` ou `tauri://localhost/preview`) pour isoler du app origin.
- Markdown rendu: rendu côté UI (marked + sanitize).
- Sortie Python textuelle: préformatée dans le panneau.
- Graphiques Python: le runtime exporte en **HTML (plotly) ou PNG (matplotlib)** stocké dans `workspace/artifacts/`, affiché dans le panneau. Pas d'exécution Python dans la preview V1.

## D7 — Fallback remote → local

**Décision: stratégie "auto" par défaut avec healthcheck proactif.**

1. Au démarrage et toutes 30s en mode `auto`: healthcheck `GET {remote}/health` (timeout 2s).
2. Si healthy + token valide: provider = remote. Banner UI/CLI: "Remote (server — model X)".
3. Si échec (timeout/5xx/auth): switch provider = local daemon. Banner: "Local (Ollama — model Y) — remote indisponible".
4. Si local daemon absent: proposer démarrage auto (`provider-supervisor`).
5. Reprise: quand remote revient, proposer (pas forcer) le retour au remote.
6. **Transparence:** toute action exécutée distante vs locale est étiquetée dans l'UI et les logs. Jamais d'ambiguïté sur le lieu d'exécution.

## D8 — Authentification

**Décision: token API (bearer) V1; SSO/mTLS en V1.1/V2.**

- V1: `Authorization: Bearer <token>` sur remote-server. Token lié à un user, rotatable, expirable.
- Login: `locaryn login --server URL --user U` → POST /auth/login → token stocké dans OS keychain (ou `~/.locaryn/credentials.toml` chiffré OS-AGNOSTIC fallback).
- Rotation: `locaryn token rotate`.
- Remote-server: hash Argon2id des tokens, stockage en SQLite, audit log de chaque usage.
- V1.1: mTLS optionnel (certificat client) pour homelab/enterprise.
- V2: OIDC/SAML SSO pour enterprise.

## D9 — TLS

**Décision: TLS obligatoire sur remote-server (rustls); loopback sans TLS pour le daemon local.**

- Remote-server: TLS 1.3 (rustls), certificat fourni (file/env) ou Let's Encrypt via challenge DNS/HTTP pour déploiements public. mTLS optionnel V1.1.
- Daemon local: HTTP plain sur `127.0.0.1:7474` (loopback only, pas de TLS — surcharge inutile en local).
- **Jamais** d'exposition du daemon local sur 0.0.0.0 sans TLS + auth. Bind explicite loopback.

## D10 — Exposition réseau

**Décision:**
- Daemon: `127.0.0.1:7474` only. Refuse les connexions non-loopback.
- Remote-server: `0.0.0.0:7473` + TLS + auth + rate limit. Recommandé derrière reverse proxy (Caddy/Traefik) ou Tailscale/Headscale pour homelab.
- Provider-supervisor + moteurs locaux: `127.0.0.1` only (11434/8080/1234/8000 selon le runtime).

## D11 — Plugins/Extensions

**Décision: système first-class, manifest `plugin.json`, 4 scopes, permissions, hot-reload.** Détail en `09-extension-model.md`.

- Format Locaryn natif `plugin.json` (schema versionné).
- Scopes: `global` (`~/.locaryn/plugins/`), `user` (même, alias), `workspace` (`.locaryn/plugins/`), `session` (transitoire).
- Permissions: `shell`, `files`, `network`, `extensions`, `mcp`, `preview`, `lsp` — déclarées dans le manifest, approuvées à l'install.
- Hot-reload via `notify` (fs watcher) + registry versionné.
- Registre local V1; catalogue/marketplace V2.

## D12 — MCP

**Décision: MCP standard (spec 2026-07-28) via `locaryn-mcp` (rmcp).**

- **Compatible directement avec le standard MCP:** tools, resources, prompts, tasks, `server/discover`, JSON Schema 2020-12, transport stateless HTTP + stdio.
- **Nécessite un adaptateur:** les features dépréciées (Roots, Sampling, Logging) sont mappées vers nos APIs (rules runtime, direct API, OpenTelemetry) quand pertinent.
- **Spécifique Locaryn:** le manifest plugin Locaryn encapsule un MCP server (déclaré dans `plugin.json` + `.mcp.json`), avec permissions et scope — au-delà du standard MCP qui ne définit pas de packaging ni de permissions.
- Registre MCP par scope (global/user/workspace) via `.locaryn/mcp.json` (format compatible Claude Code/Cursor: `mcpServers: {name: {command, args, env}}`).

## D13 — Compatibilité Claude Code / Antigravity / Cursor / Continue / Cline

**Décision: couche d'import dans `locaryn-extensions`.**

| Source | Concept importé | Mapping Locaryn |
| --- | --- | --- |
| Claude Code `.claude/agents/*.md` | subagent (frontmatter name/description/tools/model) | `agent_profiles` (même frontmatter + permissions) |
| Claude Code `.claude/commands/*.md` | slash command | `slash_commands` (même markdown + variables $0,$1) |
| Claude Code `.claude/skills/*/SKILL.md` | skill (YAML frontmatter) | `skills` (idem) |
| Claude Code `hooks.json` (PreToolUse/PostToolUse/Stop/...) | hooks | `hooks` (mêmes events + `${LOCARYN_PLUGIN_ROOT}`) |
| Claude Code `output-styles/*.md` | output style | `agent_profiles.output_style` |
| Claude Code `CLAUDE.md` / `rules/*.md` | instructions/rules | `workspace_rules` (markdown agrégé) |
| Cursor `.cursor/mcp.json` | MCP registry | `mcp_servers` (format identique) |
| Cursor `.cursor/rules/*.md` | rules | `workspace_rules` |
| Continue `config.yaml` (models/mcpServers/prompts) | config | mapping models→providers, mcpServers→mcp_servers, prompts→slash_commands |
| Cline/Roo `AGENTS.md` + `.roo/rules-*` | rules + modes | `workspace_rules` + `agent_profiles` |
| Antigravity `antigravity.yaml` | persona + toolsets | `agent_profiles` + permissions |

**Compatible directement:** MCP `.mcp.json`, markdown rules, slash commands markdown, agent frontmatter.
**Adaptateur nécessaire:** Continue `config.yaml` (YAML→Locaryn TOML/JSON), hooks Claude Code (events aliasés).
**Spécifique Locaryn:** manifest `plugin.json` avec permissions + packaging + scope — non couvert par les formats ci-dessus, qui restent des concepts importables mais pas des bundles signés.

## Tableau récapitulatif des décisions

| # | Sujet | Décision |
| --- | --- | --- |
| D1 | Desktop | Tauri v2 + React/TS |
| D2 | Daemon | Rust (crates partagés) |
| D3 | Remote-server | Rust (même crates + BSL enterprise) |
| D4 | Transport | HTTP/1.1 + SSE (MCP suit sa propre spec) |
| D5 | Stockage | SQLite + filesystem |
| D6 | Preview | iframe sandboxed + CSP strict |
| D7 | Fallback | auto: healthcheck 30s, switch propre, banner transparent |
| D8 | Auth | Bearer token V1, rotation, keychain; mTLS V1.1, SSO V2 |
| D9 | TLS | rustls obligatoire remote; plain loopback daemon |
| D10 | Réseau | daemon loopback only; remote 0.0.0.0+TLS+auth+RL |
| D11 | Plugins | plugin.json, 4 scopes, permissions, hot-reload |
| D12 | MCP | standard 2026-07-28 via rmcp + encapsulation Locaryn |
| D13 | Compat écosystème | import layer dans locaryn-extensions |
