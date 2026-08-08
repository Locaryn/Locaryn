# 01 — Product Spec

> Lochor — open-core agentic coding platform.

## Personas

| Persona | Description | Primary surface | Key needs |
| --- | --- | --- | --- |
| **Indie dev / OSS maintainer** | Solo developer, runs everything on a laptop, may have a small GPU box | Desktop + CLI, local daemon, Ollama | Free, local-first, no account, plugin/MCP tinkering, reproducible context |
| **Power user / tinkerer** | Self-hosts everything, homelab, multiple machines | Desktop + CLI, remote-server on homelab box, Tailscale/Headscale | mTLS/VPN, multiple projects, own LLM endpoints, audit logs, scripting |
| **Small team (3–15)** | Startup or R&D squad sharing one codebase | Desktop + CLI, one shared remote-server | Shared sessions/context, per-project rules, shared MCP/plugin bundle, SSO-ish |
| **Enterprise team on DGX Spark** | 20+ engineers, large repos, NVIDIA DGX Spark cluster | Desktop + CLI, enterprise remote-server module | Cross-team context/file sharing optimized for huge codebases, governance, DGX orchestration, RBAC, audit, mTLS |
| **Plugin/extension author** | Builds MCP servers, skills, agents, hooks | plugin-sdk, CLI for testing | Stable SDK, manifest schema, sandbox, hot-reload, scope model |
| **Platform/SRE** | Operates the remote-server for an org | remote-server binary, container, systemd service | TLS, healthchecks, rotation, rate limiting, logs, backups |

## User stories (representative)

### Local-first (indie)
- US-01: As an indie dev, I open Lochor desktop, pick a project, and chat with the agent that reads/edits my files and runs commands — all locally via Ollama, no account.
- US-02: As an indie dev, I run `lochor chat` from my terminal in a repo and continue the exact same session I had open in the desktop app.
- US-03: As a tinkerer, I add an MCP server via `.lochor/mcp.json` and it appears in both desktop and CLI without restart (hot-reload).
- US-04: As a tinkerer, I write a slash command in `.lochor/commands/refactor.md` and invoke `/refactor extract-module` from both surfaces.

### Remote + fallback
- US-05: As a team member, I connect in `auto` mode; Lochor tries the team remote-server first, and if it's down, transparently falls back to my local daemon, telling me which provider is active.
- US-06: As a team member, the remote-server hosts a stronger model; when it's reachable I get better quality, and my local daemon keeps working when I'm offline.
- US-07: As a platform/SRE, I deploy the remote-server as a systemd service with TLS and an API key; the desktop clients connect with URL + token.

### Enterprise (DGX Spark)
- US-08: As an enterprise engineer, I work on a 2M-LOC monorepo; the enterprise remote-server shares pre-indexed project context across the team so the agent is productive for everyone on day one.
- US-09: As an enterprise lead, I enforce workspace rules and a curated MCP/plugin bundle at the org scope; team members cannot override security-critical rules.

### Extension author
- US-10: As an extension author, I publish a plugin bundle containing a skill, a slash command, and an MCP server, scoped to `user`, with a manifest declaring permissions; Lochor prompts the user to approve on install.
- US-11: As an extension author, I import a Claude-Code-style bundle (`.claude/agents/*.md`, `commands/*.md`, `skills/*/SKILL.md`) into Lochor's format via a one-shot converter.

## Parcours principaux

1. **First run (local):** Install → `lochor init` (or open desktop) → detect Ollama on loopback → if absent, propose `lochor provider start ollama` → pick project → chat.
2. **First run (remote):** Install → configure `~/.lochor/config.toml` with server URL + token → `lochor login` → fetch authorized projects → chat against remote providers, fallback local.
3. **Continue session across surfaces:** Open desktop → resume session S → close. Open CLI in same project → `lochor chat --resume S` → identical context.
4. **Generate + preview artifact:** Ask for a small web tool → agent emits HTML/CSS/JS artifact → right panel renders it sandboxed → iterate live.
5. **Install plugin:** `lochor plugin install ./my-plugin` → manifest validation → permission prompt → activate in `user` scope → available in desktop + CLI.
6. **Fallback drill:** Unplug network → `auto` mode detects remote-server health failure → switches to local daemon → banner: "Local mode (Ollama)".

## Cas limites

- Remote-server reachable but **provider behind it down**: gateway returns 503 with provider detail; client falls back to local.
- Local daemon **not running** when CLI starts: CLI auto-spawns it (or points to the in-process core if `--no-daemon`).
- Ollama installed but **no model pulled**: provider-supervisor detects empty `/api/tags` and offers `ollama pull <recommended>`.
- Concurrent desktop + CLI on same project: daemon serializes file writes; both see the same event stream.
- Plugin declares a permission the user never grants: feature stays disabled, agent informed in system prompt.
- MCP server crashes mid-call: client surfaces a structured error, offers restart, marks tool unavailable.
- Preview artifact tries `fetch()` to external host: blocked by sandbox CSP unless `network` permission granted.
- Enterprise concurrent-client gate hit: new client gets a clear 402/429 with upgrade guidance, existing sessions unaffected.
- mTLS cert expired: client shows explicit error with renewal hint, does **not** silently fall back to plaintext.

## Non-objectifs (V1)

- Not a general-purpose IDE replacement (no full project tree editing UI; Monaco is for light edits + diffs).
- Not a model-training or fine-tuning tool.
- Not a hosted SaaS by us in V1 — the remote-server is self-hosted; we may offer a managed tier later.
- Not mobile (V1). Tauri v2 mobile support exists but is out of MVP scope.
- Not a marketplace (V1). Local registry + file/URL install only. A catalog may come in V2.
- Not multi-tenant cloud hosting — the enterprise module is single-tenant-per-deployment.
- No realtime collaborative editing (OT/CRDT) in V1. Enterprise shares context/files, not live cursors.

## Exigences fonctionnelles (V1)

| ID | Exigence |
| --- | --- |
| FR-01 | Chat agentique orienté code avec tool-use (read/write files, run commands, search). |
| FR-02 | Lecture/édition de fichiers dans un projet, avec diffs et approbation. |
| FR-03 | Exécution de commandes terminal, permission-gated, capture stdout/stderr. |
| FR-04 | Génération d'artefacts (HTML/CSS/JS, markdown, scripts Python, petits outils web). |
| FR-05 | Preview live sandboxée dans le desktop (HTML/JS/CSS, markdown rendu, sortie texte Python, graphiques Python exportés HTML/PNG). |
| FR-06 | Partage d'état (sessions, projets, historique, contexte fichiers) entre desktop et CLI via daemon. |
| FR-07 | Choix provider: remote (URL+token+TLS), local (Ollama/llama.cpp/LM Studio/vLLM), auto (remote→local fallback). |
| FR-08 | Mode dégradé: détection santé remote, bascule propre vers local, démarrage auto du runtime local si besoin. |
| FR-09 | Gestion multi-projets/workspaces avec politique de confiance par projet. |
| FR-10 | Journalisation, observabilité locale, gestion d'erreurs, reprise de session. |
| FR-11 | Système d'extensions: plugins, MCP, slash commands, commands, hooks, skills, agents spécialisés, rules workspace, LSP adapters. |
| FR-12 | Chargement par scope: global/user/workspace (et session si nécessaire). |
| FR-13 | Permissions explicites par extension (shell, files, network, extensions, MCP, preview). |
| FR-14 | Hot-reload des extensions sans redémarrage complet. |
| FR-15 | Remote-server sécurisé: TLS, auth, sessions, permissions, audit, healthchecks, streaming, providers configurés côté serveur. |
| FR-16 | 3 modes connexion client: remote / local / auto. |
| FR-17 | Signalétique claire du provider actif et du lieu d'exécution (remote vs local). |
| FR-18 | Compatibilité/import de bundles type Claude Code / Cursor / Continue / Cline. |

## Exigences non fonctionnelles (V1)

| ID | Exigence |
| --- | --- |
| NFR-01 | Desktop + CLI partagent **exactement le même cœur métier** (crates Rust). Aucune logique agent dupliquée dans l'UI. |
| NFR-02 | Daemon local unique exposant une API locale stable (HTTP/SSE) + event stream. CLI et desktop parlent à ce daemon (ou embarquent le core en in-process). |
| NFR-03 | Remote-server et daemon partagent un maximum de logique métier (même crates), sans duplication. |
| NFR-04 | Windows 11 d'abord, puis macOS, puis Linux. ARM64 prévu dès le départ (builds + CI matrix). |
| NFR-05 | Sécurité: sandbox preview (CSP + iframe sandboxed), permissions explicites, séparation workspace utilisateur / runtime interne, politique de confiance par projet. |
| NFR-06 | Loopback-only pour tout moteur local brut (Ollama/llama.cpp/vLLM). Le remote-server agit en gateway sécurisée. Jamais d'exposition directe sur Internet. |
| NFR-07 | Streaming des tokens/logs/état tâche/état preview en temps réel vers desktop et CLI. |
| NFR-08 | Performances: démarrage daemon < 500ms, premier token < 1s sur provider local chaud, preview < 100ms render. |
| NFR-09 | Maintenabilité: monorepo modulaire, peu de processus au départ, extensible sans réécriture majeure. |
| NFR-10 | Open-source core (Apache-2.0); enterprise remote-server module en BSL 1.1 (change date 4 ans). |
| NFR-11 | Testable: chaque crate testable isolément; CI obligatoire (fmt, clippy -D warnings, tests, typecheck TS). |
| NFR-12 | Buildable progressivement: le MVP ne doit pas casser l'objectif final. |
