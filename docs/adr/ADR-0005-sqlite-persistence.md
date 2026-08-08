# ADR-0005 — SQLite as primary persistence

## Context
Lochor is local-first. The daemon (loopback) and the client need embedded persistence for projects, sessions, messages, tasks, artifacts, providers, runtime state, extensions, MCP servers, commands, hooks, skills, agents, rules, LSP adapters, users, auth tokens, audit logs. The remote-server needs the same schema; enterprises with high load may later need a heavier DB.

## Decision
- **SQLite (via sqlx, `runtime-tokio-rustls`)** as the primary persistence for daemon, client, and remote-server.
- Migrations versionned in `migrations/` applied at startup via `sqlx::migrate!`.
- WAL mode for concurrency (desktop + CLI on the same daemon).
- `lochor-storage` abstracts repository access so a **PostgreSQL** backend can be added in V2 for the enterprise remote-server without rewriting callers.
- **Filesystem** for workspace artifacts, plugin files, rules markdown — not in DB (only paths/metadata are).
- **OS keychain** for secrets (provider API keys, auth tokens plaintext on client) — DB stores only references (`keychain:lochor/...`).

## Consequences
- **Positive:** Zero-config local install; single-file DB (easy backup via `VACUUM INTO`); good enough concurrency with WAL; same schema local and remote; trivial CI (in-memory or file SQLite).
- **Negative:** SQLite has write-concurrency limits (single writer) — fine for a single-team remote-server V1, but enterprises with many concurrent writers will need PostgreSQL (V2, abstraction ready).
- **Neutral:** sqlx compile-time checked queries require a DB at build time or `query!` macros with offline data (`sqlx prepare`).

## Alternatives considered
- **PostgreSQL from day 1:** rejected — heavy for local-first daemon; requires a server process; against the "few processes" constraint.
- **DuckDB:** rejected — analytical focus, less suited for OLTP session/message workload.
- **Sled / redb (embedded KV):** rejected — would require building a relational layer ourselves; SQL is the right tool for our schema.

## References
- `docs/architecture/07-persistence.md`
- `docs/architecture/03-tech-decisions.md` (D5)
