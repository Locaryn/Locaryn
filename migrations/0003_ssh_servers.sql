-- Lochor SSH server connector schema (V0.1)
--
-- Stores SSH server connections managed by the SSH connector. No column ever
-- holds a password or private key: only `secret_ref` (an OS-keychain key) and
-- `key_path` (a reference to an on-disk private key). Conventions mirror
-- `providers` / `mcp_servers`: TEXT UUID PK, lowercase enum tokens, INTEGER
-- bools, RFC-3339 TEXT timestamps, JSON-as-TEXT, UNIQUE(name, scope).

CREATE TABLE IF NOT EXISTS ssh_servers (
    id                TEXT PRIMARY KEY,               -- UUID
    name              TEXT NOT NULL,
    description       TEXT NOT NULL DEFAULT '',       -- probe-seeded, AI-evolved
    host              TEXT NOT NULL,
    port              INTEGER NOT NULL DEFAULT 22,
    username          TEXT NOT NULL,
    auth_method       TEXT NOT NULL,                  -- 'password' | 'key' | 'agent'
    secret_ref        TEXT,                           -- Keychain key ONLY; NULL for 'agent'/no passphrase
    key_path          TEXT,                           -- on-disk private key path, 'key' auth
    jump_json         TEXT,                           -- optional ProxyJump: JSON {host,port,username,auth_method,key_path}
    host_key_algo     TEXT,                           -- e.g. 'ssh-ed25519' (public data, inline)
    host_key_sha256   TEXT,                           -- base64 SHA-256 fingerprint (TOFU pin)
    host_key_verified INTEGER NOT NULL DEFAULT 0,
    ai_access         TEXT NOT NULL DEFAULT 'none',   -- 'none' | 'read_only' | 'approval' | 'trusted'
    capabilities      TEXT,                           -- JSON probe result
    scope             TEXT NOT NULL DEFAULT 'user',   -- ExtensionScope token
    status            TEXT NOT NULL DEFAULT 'unknown',-- unknown | ok | error
    enabled           INTEGER NOT NULL DEFAULT 1,
    last_connected_at TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    UNIQUE(name, scope)
);
CREATE INDEX IF NOT EXISTS idx_ssh_servers_host ON ssh_servers(host, port);
