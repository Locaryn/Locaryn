-- Lochor initial schema (V0.1)
-- Applied at daemon / remote-server startup via sqlx::migrate!.

-- ===== Projects =====
CREATE TABLE IF NOT EXISTS projects (
    id           TEXT PRIMARY KEY,
    path         TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    trust_level  TEXT NOT NULL DEFAULT 'untrusted',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    deleted_at   TEXT
);
CREATE INDEX IF NOT EXISTS idx_projects_path ON projects(path);

-- ===== Sessions =====
CREATE TABLE IF NOT EXISTS sessions (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title           TEXT,
    provider_id     TEXT,
    model           TEXT,
    created_at      TEXT NOT NULL,
    last_message_at TEXT,
    closed_at       TEXT
);
CREATE INDEX IF NOT EXISTS idx_sessions_project ON sessions(project_id);

-- ===== Messages =====
CREATE TABLE IF NOT EXISTS messages (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role          TEXT NOT NULL,
    content       TEXT NOT NULL,
    tool_calls    TEXT,
    tool_call_id  TEXT,
    tokens_in     INTEGER NOT NULL DEFAULT 0,
    tokens_out    INTEGER NOT NULL DEFAULT 0,
    parent_id     TEXT,
    created_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, created_at);

-- ===== Tasks =====
CREATE TABLE IF NOT EXISTS tasks (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    status      TEXT NOT NULL DEFAULT 'pending',
    progress    REAL NOT NULL DEFAULT 0.0,
    started_at  TEXT,
    ended_at    TEXT,
    error       TEXT
);
CREATE INDEX IF NOT EXISTS idx_tasks_session ON tasks(session_id);

-- ===== Artifacts =====
CREATE TABLE IF NOT EXISTS artifacts (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,
    path        TEXT NOT NULL,
    title       TEXT,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_artifacts_session ON artifacts(session_id);

-- ===== Providers =====
CREATE TABLE IF NOT EXISTS providers (
    id         TEXT PRIMARY KEY,
    kind       TEXT NOT NULL,
    engine     TEXT NOT NULL,
    endpoint   TEXT NOT NULL,
    model      TEXT,
    is_active  INTEGER NOT NULL DEFAULT 0,
    status     TEXT NOT NULL DEFAULT 'unknown',
    config     TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- ===== Runtime state (local engine processes) =====
CREATE TABLE IF NOT EXISTS runtime_state (
    id             TEXT PRIMARY KEY,
    engine         TEXT NOT NULL,
    pid            INTEGER,
    port           INTEGER,
    endpoint       TEXT,
    status         TEXT NOT NULL,
    last_heartbeat TEXT,
    started_at     TEXT,
    stopped_at     TEXT
);

-- ===== Users (remote-server; local daemon: single user 'local') =====
CREATE TABLE IF NOT EXISTS users (
    id           TEXT PRIMARY KEY,
    username     TEXT NOT NULL UNIQUE,
    display_name TEXT,
    role         TEXT NOT NULL DEFAULT 'developer',
    created_at   TEXT NOT NULL,
    disabled_at  TEXT
);

-- ===== Auth tokens =====
CREATE TABLE IF NOT EXISTS auth_tokens (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL,
    label       TEXT,
    scopes      TEXT,
    expires_at  TEXT,
    created_at  TEXT NOT NULL,
    revoked_at  TEXT
);
CREATE INDEX IF NOT EXISTS idx_tokens_user ON auth_tokens(user_id);

-- ===== Audit logs (remote-server) =====
CREATE TABLE IF NOT EXISTS audit_logs (
    id          TEXT PRIMARY KEY,
    user_id     TEXT,
    event       TEXT NOT NULL,
    target      TEXT,
    details     TEXT,
    ip          TEXT,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_event ON audit_logs(event, created_at);
CREATE INDEX IF NOT EXISTS idx_audit_user ON audit_logs(user_id, created_at);
