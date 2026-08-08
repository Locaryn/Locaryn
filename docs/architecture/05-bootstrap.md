# 05 — Bootstrap

Ce document décrit le squelette initial du monorepo généré dans cette session et
comment le construire/lancer en local.

## Ce qui est généré

| Zone | Fichiers | État |
| --- | --- | --- |
| Racine | `README.md`, `LICENSE`, `LICENSES.md`, `.gitignore`, `.editorconfig`, `Cargo.toml` (workspace), `rust-toolchain.toml`, `.cargo/config.toml`, `deny.toml`, `package.json`, `pnpm-workspace.yaml`, `biome.json` | ✅ Buildable |
| `apps/cli` | `Cargo.toml`, `src/main.rs` (clap, parle au daemon via sdk) | ✅ `cargo build` |
| `apps/desktop` | Tauri v2 + React/TS + Vite + 4 panneaux (mock) | ✅ `pnpm tauri dev` |
| `services/daemon` | `Cargo.toml`, `src/main.rs` (axum loopback :7474, /health, /v1/sessions) | ✅ `cargo run` |
| `services/remote-server` | `Cargo.toml`, `src/main.rs` (axum TLS :7473, /health, /auth) + `enterprise/` (BSL stub) | ✅ `cargo build` |
| `services/provider-supervisor` | `Cargo.toml`, `src/main.rs` (detect Ollama, healthcheck) | ✅ `cargo build` |
| `packages/*` (16 crates) | `Cargo.toml` + `src/lib.rs` (types stubs buildables) | ✅ `cargo check --workspace` |
| `packages-ui/*` | `core`, `chat`, `preview`, `terminal` (React/TS) | ✅ `pnpm typecheck` |
| `docs/architecture/` | 10 documents (cette spec) | ✅ |
| `docs/adr/` | 5 ADR | ✅ |
| `examples/` | plugin Lochor complet + mcp.json + SKILL.md + command.md + agent.md + hooks.json + workspace-rules.md | ✅ |
| `migrations/` | 0001_init.sql + 0002_extensions.sql | ✅ |
| `.github/workflows/ci.yml` | CI complète | ✅ |

## Prérequis

- **Rust 1.83+** (via `rustup`; `rust-toolchain.toml` pinne la version).
- **Node 22+** et **pnpm 9+** (`corepack enable`).
- **Tauri v2 prerequisites** (WebView2 runtime sur Win11 — déjà présent; sur Linux: `webkit2gtk-4.1`, `libayatana-appindicator3-dev`; sur macOS: Xcode CLT).
- **Ollama** (optionnel, pour test local runtime).

## Build & run local

```bash
# 1. JS deps
pnpm install

# 2. Vérif workspace Rust
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# 3. Daemon local (loopback :7474)
cargo run -p lochor-daemon
# → http://127.0.0.1:7474/health

# 4. CLI (parle au daemon)
cargo run -p lochor-cli -- chat
cargo run -p lochor-cli -- --server http://127.0.0.1:7474 status

# 5. Desktop (dev)
cd apps/desktop && pnpm tauri dev

# 6. Remote-server (build only en MVP — TLS cert requis pour run)
cargo build -p lochor-remote-server

# 7. Provider-supervisor
cargo run -p lochor-provider-supervisor -- status
```

## Vérifications minimales

| Vérif | Commande | Attendu |
| --- | --- | --- |
| Workspace compile | `cargo check --workspace` | 0 erreur |
| Clippy propre | `cargo clippy --workspace --all-targets -- -D warnings` | 0 warning |
| TS typecheck | `pnpm typecheck` | 0 erreur |
| Lint biome | `pnpm lint` | 0 erreur |
| Daemon santé | `curl http://127.0.0.1:7474/health` | `{"status":"ok",...}` |
| CLI status | `lochor status` | provider actif, mode |

## Ce qui n'est PAS encore implémenté (MVP reste à coder)

- Logique agentique réelle (tool-use loop, planning) — `lochor-agent-runtime` squelette.
- Provider-supervisor réel (spawn Ollama/llama-server) — stub détection only.
- MCP runtime réel (rmcp wiring) — `lochor-mcp` squelette.
- Persistence réelle (sqlx migrations appliquées) — `lochor-storage` interface.
- Remote-server TLS/auth réels — `lochor-auth` interface.
- Hot-reload extensions — `lochor-extensions` registry skeleton.
- Preview live wiring — UI panel mock.

Le squelette est **structurellement complet et buildable**, prêt à être rempli
itérativement selon la roadmap (`10-roadmap.md`).
