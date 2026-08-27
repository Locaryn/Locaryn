# 06 — API Contract

Contrat **unique** partagé entre le daemon local et le remote-server. Le remote-server ajoute `/auth`, TLS, audit, et les endpoints enterprise. Le `locaryn-sdk` parle les deux indifféremment (seul l'endpoint change).

## Conventions

- Base: `http://127.0.0.1:7474/v1` (daemon) ou `https://<host>:7473/v1` (remote).
- Auth: `Authorization: Bearer <token>` (remote only; daemon loopback sans auth V1).
- Content-Type: `application/json` sauf streaming (`text/event-stream`).
- Streaming: **SSE** (`event: <type>\ndata: <json>\n\n`).
- Erreurs: `{ "error": { "code": "...", "message": "...", "details": {} } }` + HTTP code.

## Endpoints core (daemon + remote)

### Health & mode

| Méthode | Path | Rôle | Local | Remote |
| --- | --- | --- | --- | --- |
| GET | `/health` | Santé du service + provider actif | ✅ | ✅ |
| GET | `/v1/info` | Version, capacités, mode (local/remote), provider actif | ✅ | ✅ |

### Projects & workspaces

| Méthode | Path | Corps/Réponse | Local/Remote |
| --- | --- | --- | --- |
| GET | `/v1/projects` | Liste projects | both |
| POST | `/v1/projects` | `{ path, name, trust_level }` → `Project` | both |
| GET | `/v1/projects/{id}` | Détail project | both |
| PATCH | `/v1/projects/{id}` | Update trust_level, name | both |
| DELETE | `/v1/projects/{id}` | Supprime (soft) | both |

### Sessions & messages

| Méthode | Path | Rôle | Local/Remote |
| --- | --- | --- | --- |
| GET | `/v1/projects/{pid}/sessions` | Liste | both |
| POST | `/v1/projects/{pid}/sessions` | Crée session | both |
| GET | `/v1/sessions/{id}` | Détail + messages | both |
| POST | `/v1/sessions/{id}/messages` | Envoie message → déclenche agent (SSE stream) | both |
| POST | `/v1/sessions/{id}/cancel` | Annule tâche en cours | both |
| GET | `/v1/sessions/{id}/artifacts` | Artefacts de la session | both |

### Streaming events (SSE)

`POST /v1/sessions/{id}/messages` ouvre un flux SSE:

```
event: message.start
data: {"message_id":"...", "task_id":"..."}

event: token
data: {"text":"Hello"}

event: tool_call
data: {"tool":"read_file","args":{"path":"src/main.rs"},"call_id":"..."}

event: tool_approval
data: {"call_id":"...","tool":"run_command","args":{"cmd":"rm -rf /"},"risk":"high"}
   # client approuve via POST /v1/tasks/{task_id}/approve {call_id, decision}

event: tool_result
data: {"call_id":"...","ok":true,"output":"..."}

event: artifact
data: {"artifact_id":"...","kind":"html","path":"artifacts/foo.html"}

event: task.update
data: {"task_id":"...","status":"running","progress":0.4}

event: preview.update
data: {"artifact_id":"...","url":"locaryn-preview://artifact/foo.html"}

event: provider.changed
data: {"provider":"local","engine":"ollama","model":"qwen2.5-coder:7b","reason":"remote_unavailable"}

event: log
data: {"level":"info","msg":"...","source":"agent"}

event: message.end
data: {"message_id":"...","tokens_in":120,"tokens_out":450,"duration_ms":8200}
```

### Tasks (long-running)

| Méthode | Path | Rôle |
| --- | --- | --- |
| GET | `/v1/tasks/{id}` | Statut tâche |
| POST | `/v1/tasks/{id}/cancel` | Annule |
| POST | `/v1/tasks/{id}/approve` | Approuve tool_call `{call_id, decision: "allow"\|"deny", scope: "once"\|"session"\|"project"}` |

**Statuts tâche:** `pending` → `running` → `awaiting_approval` → `running` → `completed` \| `cancelled` \| `failed`.

### Artifacts

| Méthode | Path | Rôle |
| --- | --- | --- |
| GET | `/v1/artifacts/{id}` | Métadonnées + contenu |
| GET | `/v1/artifacts/{id}/raw` | Contenu brut (HTML/JS/PNG...) |
| POST | `/v1/sessions/{id}/artifacts` | Crée artefact |

### Files (project workspace)

| Méthode | Path | Rôle | Permission |
| --- | --- | --- | --- |
| GET | `/v1/projects/{pid}/files?path=` | Lit fichier | `files.read` |
| PUT | `/v1/projects/{pid}/files?path=` | Écrit fichier (diff inclus) | `files.write` + approval |
| POST | `/v1/projects/{pid}/search` | Recherche (ripgrep) | `files.read` |

### Terminal

| Méthode | Path | Rôle | Permission |
| --- | --- | --- | --- |
| POST | `/v1/projects/{pid}/exec` | Exécute commande (SSE stdout/stderr/exit) | `shell` + approval si risque |

### Providers

| Méthode | Path | Rôle | Local/Remote |
| --- | --- | --- | --- |
| GET | `/v1/providers` | Liste providers configurés + status | both |
| GET | `/v1/providers/{id}/health` | Healthcheck provider | both |
| POST | `/v1/providers/active` | Bascule provider actif `{id}` ou `{mode: "auto"\|"local"\|"remote"}` | both |
| POST | `/v1/providers/local/start` | Démarre runtime local (`engine: "ollama"\|"llama_cpp"\|"lmstudio"\|"vllm"`, `model`) | local only |
| POST | `/v1/providers/local/stop` | Arrête runtime local | local only |

## Endpoints remote-only

### Auth

| Méthode | Path | Rôle |
| --- | --- | --- |
| POST | `/v1/auth/login` | `{user, password}` → `{token, expires_at}` (ou 2FA challenge) |
| POST | `/v1/auth/refresh` | Rotate token |
| POST | `/v1/auth/logout` | Invalide token |
| GET | `/v1/auth/me` | Profil + scopes |
| GET | `/v1/auth/audit` | Audit log (admin only) |

### Enterprise (BSL)

| Méthode | Path | Rôle |
| --- | --- | --- |
| GET | `/v1/enterprise/context/{project_id}` | Contexte partagé pré-indexé |
| POST | `/v1/enterprise/context/{project_id}/index` | Ré-indexe le projet pour collab |
| GET | `/v1/enterprise/dgx/spark/status` | Statut cluster DGX Spark |
| POST | `/v1/enterprise/dgx/spark/schedule` | Planifie un job inference |

## Endpoints extensions (daemon + remote, selon permissions)

### Plugins

| Méthode | Path | Rôle | Local/Remote |
| --- | --- | --- | --- |
| GET | `/v1/extensions` | Liste extensions installées (tous kinds) | both |
| POST | `/v1/extensions/install` | `{source: "path"\|"url", scope: "global"\|"user"\|"workspace"}` | both |
| POST | `/v1/extensions/{id}/enable` | Active | both |
| POST | `/v1/extensions/{id}/disable` | Désactive | both |
| DELETE | `/v1/extensions/{id}` | Désinstalle | both |
| GET | `/v1/extensions/{id}/permissions` | Permissions demandées vs accordées | both |
| POST | `/v1/extensions/{id}/permissions` | Approuve/refuse permissions | both |
| POST | `/v1/extensions/reload` | Hot-reload (ou automatique via fs watcher) | both |
| GET | `/v1/capabilities` | Liste canonique des capacités reconnues par ce serveur | both |

### MCP servers

| Méthode | Path | Rôle |
| --- | --- | --- |
| GET | `/v1/mcp/servers` | Liste MCP servers enregistrés (par scope) |
| POST | `/v1/mcp/servers` | Enregistre (`{name, command, args, env, scope, transport}`) |
| DELETE | `/v1/mcp/servers/{name}` | Désenregistre |
| POST | `/v1/mcp/servers/{name}/start` | Démarre le server |
| POST | `/v1/mcp/servers/{name}/stop` | Arrête |
| GET | `/v1/mcp/servers/{name}/discover` | `server/discover` MCP (tools/resources/prompts) |
| POST | `/v1/mcp/servers/{name}/tools/{tool}` | Invoque tool MCP |

### Commands & slash commands

| Méthode | Path | Rôle |
| --- | --- | --- |
| GET | `/v1/commands` | Liste commands + slash commands (par scope) |
| POST | `/v1/commands/{name}/invoke` | `{args}` → exécute (SSE output) |

### Hooks

| Méthode | Path | Rôle |
| --- | --- | --- |
| GET | `/v1/hooks` | Liste hooks enregistrés |
| (interne) | hook runtime | Invoqué par le core avant/après actions; pas exposé directement sauf debug |

### Skills

| Méthode | Path | Rôle |
| --- | --- | --- |
| GET | `/v1/skills` | Liste skills (par scope) |
| POST | `/v1/skills/{name}/invoke` | `{input}` → exécute skill (injecte system prompt + run) |

### Agents spécialisés

| Méthode | Path | Rôle |
| --- | --- | --- |
| GET | `/v1/agents` | Liste agent profiles |
| POST | `/v1/sessions/{id}/delegate` | Délègue à un subagent `{agent: "code-reviewer", task: "..."}` (SSE) |

### Workspace rules

| Méthode | Path | Rôle |
| --- | --- | --- |
| GET | `/v1/projects/{pid}/rules` | Règles agrégées (global + workspace) |
| PUT | `/v1/projects/{pid}/rules` | Met à jour règles workspace |

### LSP adapters

| Méthode | Path | Rôle |
| --- | --- | --- |
| GET | `/v1/projects/{pid}/lsp` | LSP servers actifs |
| POST | `/v1/projects/{pid}/lsp` | Enregistre LSP (`{language, command, args}`) |
| POST | `/v1/projects/{pid}/lsp/{lang}/symbols` | Query LSP (exposé comme tool agent) |

## Modèles de données (résumé — détail en `07-persistence.md`)

```jsonc
// Project
{ "id": "uuid", "path": "D:/repos/foo", "name": "foo", "trust_level": "trusted|untrusted|sandbox",
  "created_at": "...", "updated_at": "..." }

// Session
{ "id": "uuid", "project_id": "uuid", "title": "...", "created_at": "...",
  "last_message_at": "...", "provider_id": "uuid|null", "model": "qwen2.5-coder:7b" }

// Message
{ "id": "uuid", "session_id": "uuid", "role": "user|assistant|tool|system",
  "content": "...", "tool_calls": [...], "tokens_in": 0, "tokens_out": 0, "created_at": "..." }

// Task
{ "id": "uuid", "session_id": "uuid", "status": "pending|running|awaiting_approval|completed|cancelled|failed",
  "progress": 0.0, "started_at": "...", "ended_at": "...", "error": "..." }

// Artifact
{ "id": "uuid", "session_id": "uuid", "kind": "html|markdown|python_text|image_png|plotly_html",
  "path": "artifacts/.../foo.html", "title": "...", "created_at": "..." }

// Provider
{ "id": "uuid", "kind": "remote|local", "engine": "ollama|llama_cpp|lmstudio|vllm|openai_compat",
  "endpoint": "http://127.0.0.1:11434", "model": "...", "status": "unknown|healthy|unhealthy|starting",
  "is_active": true }

// Extension (tous kinds unifiés)
{ "id": "uuid", "name": "my-plugin", "version": "1.0.0", "api_version": "0.1",
  "scope": "global|user|workspace", "kind": "plugin|mcp|command|skill|hook|agent|rules|lsp",
  "enabled": true, "permissions": { ... }, "manifest_path": "..." }
```

## Permissions / scopes

### Permissions extension (déclarées dans `morph.json`)

| Permission | Description |
| --- | --- |
| `shell` | Exécuter commandes terminal |
| `files.read` | Lire fichiers workspace |
| `files.write` | Écrire fichiers workspace |
| `network` | Accès réseau (fetch, MCP HTTP) |
| `extensions` | Charger/gérer extensions |
| `mcp` | Activer outils MCP |
| `preview` | Ouvrir/exécuter artefacts en preview |
| `lsp` | Enregistrer/adresser LSP |
| `env` | Lire vars d'env |

### Scopes utilisateur (RBAC remote-server)

| Rôle | Capacités |
| --- | --- |
| `viewer` | Lecture sessions/projects |
| `developer` | + chat, exec, files, extensions (workspace) |
| `maintainer` | + extensions (user), providers config |
| `admin` | + users, audit, enterprise module |

## Audit events (remote-server)

| Event | Origine |
| --- | --- |
| `auth.login`, `auth.logout`, `auth.token.rotated` | auth |
| `provider.switch`, `provider.local.start`, `provider.local.stop` | providers |
| `exec.command` | terminal |
| `extension.install`, `extension.enable`, `extension.disable`, `extension.remove`, `extension.permission.granted` | extensions |
| `mcp.server.start`, `mcp.server.stop`, `mcp.tool.invoke` | mcp |
| `session.create`, `session.message`, `session.artifact` | sessions |
| `enterprise.context.index`, `enterprise.dgx.schedule` | enterprise |

## Tableau local-only / remote-only / shared

| Capacité | Local only | Remote only | Shared (both) |
| --- | --- | --- | --- |
| Daemon loopback API | ✅ | | |
| Démarrer runtime local (Ollama/llama.cpp...) | ✅ | | |
| Provider-supervisor | ✅ | | |
| TLS + auth bearer | | ✅ | |
| Audit log persistant | | ✅ | |
| RBAC users/roles | | ✅ | |
| Enterprise collab / DGX | | ✅ (BSL) | |
| Sessions, messages, tasks | | | ✅ |
| Projects, files, search | | | ✅ |
| Artifacts + preview | | | ✅ |
| Providers list + healthcheck | | | ✅ |
| Bascule provider (auto/local/remote) | | | ✅ |
| Extensions (plugins/MCP/commands/hooks/skills/agents/rules/LSP) | | | ✅ |
| Streaming SSE (tokens/logs/tasks/preview) | | | ✅ |
| Hot-reload extensions | ✅ (fs local) | ✅ (via API) | ✅ |
| MCP servers stdio | ✅ (local) | ❌ (sauf HTTP MCP) | ✅ (HTTP MCP) |
| mTLS / SSO | | ✅ (V1.1/V2) | |
