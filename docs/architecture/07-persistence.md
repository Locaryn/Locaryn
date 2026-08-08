# 07 — Persistence

SQLite (sqlx, `runtime-tokio-rustls`) pour le daemon et le client. Le remote-server utilise le même schéma (SQLite par défaut; abstraction `locaryn-storage` pour PostgreSQL en V2 enterprise).

## Localisation

- Client (daemon + desktop in-process): `~/.locaryn/data/locaryn.db`
- Remote-server: `<data_dir>/locaryn.db` (ex: `/var/lib/locaryn/locaryn.db` ou `C:\ProgramData\Locaryn\locaryn.db`)
- Workspaces: `<project_path>/.locaryn/` (règles, mcp.json, plugins, artifacts) — non stocké en DB.
- Migrations: `migrations/*.sql`, appliquées au démarrage via `sqlx::migrate!`.

## Schéma

### 0001_init.sql — cœur

```sql
-- ===== Projects =====
CREATE TABLE projects (
    id           TEXT PRIMARY KEY,            -- uuid
    path         TEXT NOT NULL UNIQUE,        -- abs path
    name         TEXT NOT NULL,
    trust_level  TEXT NOT NULL DEFAULT 'untrusted',  -- untrusted|trusted|sandbox
    created_at   TEXT NOT NULL,               -- ISO8601
    updated_at   TEXT NOT NULL,
    deleted_at   TEXT
);
CREATE INDEX idx_projects_path ON projects(path);

-- ===== Sessions =====
CREATE TABLE sessions (
    id             TEXT PRIMARY KEY,
    project_id     TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title          TEXT,
    provider_id    TEXT,                       -- nullable: provider used
    model          TEXT,
    created_at     TEXT NOT NULL,
    last_message_at TEXT,
    closed_at      TEXT
);
CREATE INDEX idx_sessions_project ON sessions(project_id);

-- ===== Messages =====
CREATE TABLE messages (
    id           TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role         TEXT NOT NULL,               -- user|assistant|tool|system
    content      TEXT NOT NULL,               -- markdown / text
    tool_calls   TEXT,                        -- JSON array
    tool_call_id TEXT,                        -- for role=tool
    tokens_in    INTEGER NOT NULL DEFAULT 0,
    tokens_out   INTEGER NOT NULL DEFAULT 0,
    parent_id    TEXT,                        -- for branching
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_messages_session ON messages(session_id, created_at);

-- ===== Tasks =====
CREATE TABLE tasks (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    status      TEXT NOT NULL DEFAULT 'pending',  -- pending|running|awaiting_approval|completed|cancelled|failed
    progress    REAL NOT NULL DEFAULT 0.0,
    started_at  TEXT,
    ended_at    TEXT,
    error       TEXT
);
CREATE INDEX idx_tasks_session ON tasks(session_id);

-- ===== Artifacts =====
CREATE TABLE artifacts (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,               -- html|markdown|python_text|image_png|plotly_html
    path        TEXT NOT NULL,               -- relative to workspace artifacts/
    title       TEXT,
    created_at  TEXT NOT NULL
);
CREATE INDEX idx_artifacts_session ON artifacts(session_id);

-- ===== Providers =====
CREATE TABLE providers (
    id         TEXT PRIMARY KEY,
    kind       TEXT NOT NULL,                -- remote|local
    engine     TEXT NOT NULL,                -- ollama|llama_cpp|lmstudio|vllm|openai_compat
    endpoint   TEXT NOT NULL,                -- loopback url
    model      TEXT,
    is_active  INTEGER NOT NULL DEFAULT 0,
    status     TEXT NOT NULL DEFAULT 'unknown',  -- unknown|healthy|unhealthy|starting
    config     TEXT,                          -- JSON: extra config (api_key ref, tls, ...)
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- ===== Runtime state (local engine processes) =====
CREATE TABLE runtime_state (
    id          TEXT PRIMARY KEY,
    engine      TEXT NOT NULL,                -- ollama|llama_cpp|lmstudio|vllm
    pid         INTEGER,
    port        INTEGER,
    endpoint    TEXT,
    status      TEXT NOT NULL,                -- starting|running|stopped|crashed
    last_heartbeat TEXT,
    started_at  TEXT,
    stopped_at  TEXT
);

-- ===== Users (remote-server; local daemon: single user 'local') =====
CREATE TABLE users (
    id          TEXT PRIMARY KEY,
    username    TEXT NOT NULL UNIQUE,
    display_name TEXT,
    role        TEXT NOT NULL DEFAULT 'developer',  -- viewer|developer|maintainer|admin
    created_at  TEXT NOT NULL,
    disabled_at TEXT
);

-- ===== Auth tokens =====
CREATE TABLE auth_tokens (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash   TEXT NOT NULL,               -- Argon2id hash
    label        TEXT,                        -- "cli-laptop", "desktop-office"
    scopes       TEXT,                        -- JSON array of permission scopes
    expires_at   TEXT,
    created_at   TEXT NOT NULL,
    revoked_at   TEXT
);
CREATE INDEX idx_tokens_user ON auth_tokens(user_id);

-- ===== Audit logs (remote-server) =====
CREATE TABLE audit_logs (
    id          TEXT PRIMARY KEY,
    user_id     TEXT,
    event       TEXT NOT NULL,                -- auth.login, exec.command, ...
    target      TEXT,                         -- entity id
    details     TEXT,                         -- JSON
    ip          TEXT,
    created_at  TEXT NOT NULL
);
CREATE INDEX idx_audit_event ON audit_logs(event, created_at);
CREATE INDEX idx_audit_user ON audit_logs(user_id, created_at);
```

### 0002_extensions.sql — système d'extensions

```sql
-- ===== Extensions (unified registry) =====
CREATE TABLE extensions (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    version       TEXT NOT NULL,
    api_version   TEXT NOT NULL,              -- Locaryn extension API version
    kind          TEXT NOT NULL,              -- plugin|mcp|command|skill|hook|agent|rules|lsp
    scope         TEXT NOT NULL,              -- global|user|workspace
    source        TEXT,                       -- path or url installed from
    manifest_path TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    UNIQUE(name, scope)
);
CREATE INDEX idx_extensions_scope_kind ON extensions(scope, kind);

-- ===== Extension installs (history) =====
CREATE TABLE extension_installs (
    id            TEXT PRIMARY KEY,
    extension_id  TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE,
    installed_by  TEXT,                       -- user id (remote) or 'local'
    scope         TEXT NOT NULL,
    status        TEXT NOT NULL,              -- installing|installed|failed|uninstalling
    log           TEXT,                       -- install log
    created_at    TEXT NOT NULL,
    completed_at  TEXT
);

-- ===== Extension permissions =====
CREATE TABLE extension_permissions (
    id            TEXT PRIMARY KEY,
    extension_id  TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE,
    permission    TEXT NOT NULL,              -- shell|files.read|files.write|network|...
    requested     INTEGER NOT NULL DEFAULT 0,
    granted       INTEGER NOT NULL DEFAULT 0,
    scope_granted TEXT,                       -- once|session|project|always
    decided_by    TEXT,                       -- user id or 'local'
    decided_at    TEXT,
    UNIQUE(extension_id, permission)
);

-- ===== MCP servers =====
CREATE TABLE mcp_servers (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    command     TEXT,                         -- for stdio transport
    args        TEXT,                         -- JSON array
    env         TEXT,                         -- JSON object
    url         TEXT,                         -- for HTTP transport (stateless)
    transport   TEXT NOT NULL,                -- stdio|http
    scope       TEXT NOT NULL,                -- global|user|workspace
    enabled     INTEGER NOT NULL DEFAULT 1,
    auto_start  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    UNIQUE(name, scope)
);

-- ===== Commands (plain commands) =====
CREATE TABLE commands (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    scope         TEXT NOT NULL,
    source_path   TEXT NOT NULL,              -- markdown file
    description   TEXT,
    allowed_tools TEXT,                       -- JSON array; tool gating
    enabled       INTEGER NOT NULL DEFAULT 1,
    UNIQUE(name, scope)
);

-- ===== Slash commands =====
CREATE TABLE slash_commands (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,              -- /refactor, /build
    scope         TEXT NOT NULL,
    source_path   TEXT NOT NULL,
    description   TEXT,
    allowed_tools TEXT,
    arguments     TEXT,                       -- JSON array of arg names
    enabled       INTEGER NOT NULL DEFAULT 1,
    UNIQUE(name, scope)
);

-- ===== Hooks =====
CREATE TABLE hooks (
    id          TEXT PRIMARY KEY,
    event       TEXT NOT NULL,                -- PreToolUse|PostToolUse|Stop|SessionStart|...
    matcher     TEXT,                         -- tool name matcher
    command     TEXT NOT NULL,                -- shell command
    timeout_ms  INTEGER NOT NULL DEFAULT 30000,
    scope       TEXT NOT NULL,
    source_path TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    UNIQUE(event, matcher, command, scope)
);

-- ===== Skills =====
CREATE TABLE skills (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    version     TEXT,
    scope       TEXT NOT NULL,
    source_path TEXT NOT NULL,                -- SKILL.md
    auto_trigger INTEGER NOT NULL DEFAULT 0,
    enabled     INTEGER NOT NULL DEFAULT 1,
    UNIQUE(name, scope)
);

-- ===== Agent profiles (specialized subagents) =====
CREATE TABLE agent_profiles (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,                -- code-reviewer, architect, ...
    description TEXT,
    model       TEXT,                         -- optional override
    tools       TEXT,                         -- JSON array (tool subset)
    system_prompt TEXT,                       -- body of the markdown
    output_style TEXT,
    scope       TEXT NOT NULL,
    source_path TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    UNIQUE(name, scope)
);

-- ===== Workspace rules =====
CREATE TABLE workspace_rules (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    scope       TEXT NOT NULL,                -- global|workspace
    source_path TEXT NOT NULL,                -- LOCARYN.md or rules/*.md
    content     TEXT NOT NULL,                -- aggregated markdown
    priority    INTEGER NOT NULL DEFAULT 0,   -- global < workspace
    enabled     INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_rules_project ON workspace_rules(project_id, scope, priority);

-- ===== LSP adapters =====
CREATE TABLE lsp_adapters (
    id          TEXT PRIMARY KEY,
    project_id  TEXT REFERENCES projects(id) ON DELETE CASCADE,
    language    TEXT NOT NULL,
    command     TEXT NOT NULL,
    args        TEXT,                         -- JSON array
    scope       TEXT NOT NULL,                -- global|user|workspace
    enabled     INTEGER NOT NULL DEFAULT 1,
    UNIQUE(project_id, language, scope)
);
```

## Migrations appliquées au démarrage

`locaryn-storage` expose `migrate(db).await` qui exécute `sqlx::migrate!("../../migrations")`. Idempotent, versionné dans `migrations/`.

## Stratégie de données

| Donnée | Locale client (toujours) | Sync/serveur (possible) | Jamais exposée par défaut |
| --- | --- | --- | --- |
| Projects, sessions, messages, tasks, artifacts | ✅ (daemon) | ✅ (remote: l'utilisateur choisit de pousser une session) | — |
| Providers config | ✅ | ✅ (config remote providers server-side) | **API keys / tokens** (stockés en keychain, jamais en clair en DB; le `config` JSON ne contient qu'une référence `keychain:provider/1`) |
| Auth tokens | ❌ (loopback daemon n'a pas d'auth) | ✅ (hash Argon2id server-side) | **Token plaintext** (jamais stocké server-side, seulement hash) |
| Audit logs | optionnel (local debug) | ✅ (remote-server persistant) | — |
| Extensions, mcp_servers, commands, hooks, skills, agents, rules, lsp | ✅ | ✅ (un bundle peut être shared via remote) | **Variables d'env des MCP servers** (peuvent contenir des secrets; masquées dans l'API sauf admin) |
| Extension permissions | ✅ | ✅ | — |
| Users, roles | ❌ (local daemon: user `local`) | ✅ (remote) | **Password hashes** (Argon2id, jamais retournés par l'API) |
| Runtime state (pid/port moteur local) | ✅ | ❌ (local only) | — |
| Workspace file contents | ✅ (filesystem) | ✅ (enterprise collab: index, pas raw) | **Fichiers hors workspace trusté** (jamais lus sans permission) |

### Règles de non-exposition

1. **API keys provider:** stockées via `locaryn-auth` dans l'OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service). La DB ne stocke qu'une référence (`keychain:locaryn/provider/<id>`). `GET /v1/providers` masque les secrets.
2. **MCP env vars:** peuvent contenir des tokens; `GET /v1/mcp/servers` retourne les clés mais masque les valeurs (`"***"`) sauf pour `admin`.
3. **Auth tokens:** jamais loggés en clair dans audit; hash du token only.
4. **Fichiers workspace:** le daemon ne lit que les chemins sous `<project_path>` du project trusté; `files.read` permission + trust level `trusted` requis pour accès complet.
5. **Preview artifacts:** servis depuis origine `locaryn-preview://` isolée, CSP strict, pas d'accès au app origin ni au filesystem.

## Backup & reprise

- Daemon: `~/.locaryn/data/locaryn.db` est un fichier SQLite standard; WAL mode activé. Backup = copie du fichier (avec `VACUUM INTO`).
- Remote-server: backup quotidien du DB + artifacts dir; rotation 30j.
- Reprise de session: une session fermée peut être rouverte (`GET /v1/sessions/{id}` recharge l'historique); les tasks interrompues sont marquées `failed` au redémarrage avec une erreur `interrupted`.
