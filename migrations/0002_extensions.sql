-- Locaryn extension system schema (V0.1)

-- ===== Extensions (unified registry) =====
CREATE TABLE IF NOT EXISTS extensions (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    version       TEXT NOT NULL,
    api_version   TEXT NOT NULL,
    kind          TEXT NOT NULL,
    scope         TEXT NOT NULL,
    source        TEXT,
    manifest_path TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    UNIQUE(name, scope)
);
CREATE INDEX IF NOT EXISTS idx_extensions_scope_kind ON extensions(scope, kind);

-- ===== Extension installs (history) =====
CREATE TABLE IF NOT EXISTS extension_installs (
    id            TEXT PRIMARY KEY,
    extension_id  TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE,
    installed_by  TEXT,
    scope         TEXT NOT NULL,
    status        TEXT NOT NULL,
    log           TEXT,
    created_at    TEXT NOT NULL,
    completed_at  TEXT
);

-- ===== Extension permissions =====
CREATE TABLE IF NOT EXISTS extension_permissions (
    id            TEXT PRIMARY KEY,
    extension_id  TEXT NOT NULL REFERENCES extensions(id) ON DELETE CASCADE,
    permission    TEXT NOT NULL,
    requested     INTEGER NOT NULL DEFAULT 0,
    granted       INTEGER NOT NULL DEFAULT 0,
    scope_granted TEXT,
    decided_by    TEXT,
    decided_at    TEXT,
    UNIQUE(extension_id, permission)
);

-- ===== MCP servers =====
CREATE TABLE IF NOT EXISTS mcp_servers (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    command     TEXT,
    args        TEXT,
    env         TEXT,
    url         TEXT,
    transport   TEXT NOT NULL,
    scope       TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    auto_start  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    UNIQUE(name, scope)
);

-- ===== Commands (plain commands) =====
CREATE TABLE IF NOT EXISTS commands (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    scope         TEXT NOT NULL,
    source_path   TEXT NOT NULL,
    description   TEXT,
    allowed_tools TEXT,
    enabled       INTEGER NOT NULL DEFAULT 1,
    UNIQUE(name, scope)
);

-- ===== Slash commands =====
CREATE TABLE IF NOT EXISTS slash_commands (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    scope         TEXT NOT NULL,
    source_path   TEXT NOT NULL,
    description   TEXT,
    allowed_tools TEXT,
    arguments     TEXT,
    enabled       INTEGER NOT NULL DEFAULT 1,
    UNIQUE(name, scope)
);

-- ===== Hooks =====
CREATE TABLE IF NOT EXISTS hooks (
    id          TEXT PRIMARY KEY,
    event       TEXT NOT NULL,
    matcher     TEXT,
    command     TEXT NOT NULL,
    timeout_ms  INTEGER NOT NULL DEFAULT 30000,
    scope       TEXT NOT NULL,
    source_path TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    UNIQUE(event, matcher, command, scope)
);

-- ===== Skills =====
CREATE TABLE IF NOT EXISTS skills (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT,
    version      TEXT,
    scope        TEXT NOT NULL,
    source_path  TEXT NOT NULL,
    auto_trigger INTEGER NOT NULL DEFAULT 0,
    enabled      INTEGER NOT NULL DEFAULT 1,
    UNIQUE(name, scope)
);

-- ===== Agent profiles =====
CREATE TABLE IF NOT EXISTS agent_profiles (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    description   TEXT,
    model         TEXT,
    tools         TEXT,
    system_prompt TEXT,
    output_style  TEXT,
    scope         TEXT NOT NULL,
    source_path   TEXT,
    enabled       INTEGER NOT NULL DEFAULT 1,
    UNIQUE(name, scope)
);

-- ===== Workspace rules =====
CREATE TABLE IF NOT EXISTS workspace_rules (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    scope       TEXT NOT NULL,
    source_path TEXT NOT NULL,
    content     TEXT NOT NULL,
    priority    INTEGER NOT NULL DEFAULT 0,
    enabled     INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_rules_project ON workspace_rules(project_id, scope, priority);

-- ===== LSP adapters =====
CREATE TABLE IF NOT EXISTS lsp_adapters (
    id          TEXT PRIMARY KEY,
    project_id  TEXT REFERENCES projects(id) ON DELETE CASCADE,
    language    TEXT NOT NULL,
    command     TEXT NOT NULL,
    args        TEXT,
    scope       TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    UNIQUE(project_id, language, scope)
);
