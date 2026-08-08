# 04 — Monorepo Plan

Monorepo unique `lochor/lochor` sur GitHub. **Un seul dépôt** (pas de multi-repo) — la contrainte "même cœur" l'impose.

## Arborescence complète

```
lochor/
├── .cargo/
│   └── config.toml
├── .github/
│   ├── workflows/
│   │   ├── ci.yml                # fmt, clippy, test, typecheck, lint, deny
│   │   ├── build-matrix.yml      # builds x64+ARM64 Win/macOS/Linux
│   │   └── release.yml           # tag-triggered release
│   └── ISSUE_TEMPLATE/
├── apps/
│   ├── cli/                      # @lochor/cli (Rust binary, clap)
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── desktop/                  # @lochor/desktop (Tauri v2 + React/TS)
│       ├── package.json
│       ├── vite.config.ts
│       ├── tsconfig.json
│       ├── index.html
│       ├── src/                  # React app (panels, Monaco, xterm, preview)
│       │   ├── main.tsx
│       │   ├── App.tsx
│       │   ├── panels/
│       │   ├── components/
│       │   └── styles/
│       └── src-tauri/
│           ├── Cargo.toml
│           ├── tauri.conf.json
│           ├── capabilities/
│           │   └── default.json
│           ├── binaries/         # sidecars (provider-supervisor per target)
│           └── src/main.rs
├── services/
│   ├── daemon/                   # lochor-daemon (Rust binary, axum, loopback)
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   ├── remote-server/            # lochor-remote-server (Rust binary, TLS, auth)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   └── main.rs
│   │   └── enterprise/           # BSL 1.1 module (collaboration, DGX, gate)
│   │       ├── Cargo.toml
│   │       └── src/lib.rs
│   └── provider-supervisor/      # lochor-provider-supervisor (Rust binary/sidecar)
│       ├── Cargo.toml
│       └── src/main.rs
├── packages/                     # Les 16 crates Rust = LE CŒUR
│   ├── shared-types/             # lochor-shared-types
│   ├── sdk/                      # lochor-sdk (client HTTP/SSE)
│   ├── auth/                     # lochor-auth
│   ├── config/                   # lochor-config
│   ├── storage/                  # lochor-storage (SQLite)
│   ├── events/                   # lochor-events (event types + bus)
│   ├── preview/                  # lochor-preview (artifact model, CSP)
│   ├── extensions/               # lochor-extensions (registry, loader, import)
│   ├── mcp/                      # lochor-mcp (rmcp wrapper)
│   ├── plugin-sdk/               # lochor-plugin-sdk (for authors)
│   ├── command-runtime/          # lochor-command-runtime
│   ├── hook-runtime/             # lochor-hook-runtime
│   ├── skill-runtime/            # lochor-skill-runtime
│   ├── agent-runtime/            # lochor-agent-runtime
│   ├── rules-runtime/            # lochor-rules-runtime
│   └── lsp-adapters/             # lochor-lsp-adapters
├── packages-ui/                  # Shared React/TS UI components
│   ├── core/                     # @lochor/ui-core (buttons, panels, tokens)
│   ├── chat/                     # @lochor/ui-chat
│   ├── preview/                  # @lochor/ui-preview
│   └── terminal/                 # @lochor/ui-terminal (xterm wrapper)
├── docs/
│   ├── architecture/
│   │   ├── 01-product-spec.md
│   │   ├── 02-architecture.md
│   │   ├── 03-tech-decisions.md
│   │   ├── 04-monorepo-plan.md
│   │   ├── 05-bootstrap.md
│   │   ├── 06-api-contract.md
│   │   ├── 07-persistence.md
│   │   ├── 08-ux.md
│   │   ├── 09-extension-model.md
│   │   └── 10-roadmap.md
│   └── adr/
│       ├── ADR-0001-rust-tauri-core.md
│       ├── ADR-0002-http-sse-transport.md
│       ├── ADR-0003-open-core-license.md
│       ├── ADR-0004-mcp-compatibility.md
│       ├── ADR-0005-sqlite-persistence.md
│       └── README.md
├── examples/
│   ├── plugins/
│   │   └── my-plugin/
│   │       ├── plugin.json
│   │       ├── skills/
│   │       ├── commands/
│   │       ├── agents/
│   │       ├── hooks/
│   │       └── mcp/
│   ├── mcp.json
│   ├── SKILL.md
│   ├── command.md
│   ├── agent.md
│   ├── hooks.json
│   └── workspace-rules.md
├── migrations/
│   ├── 0001_init.sql
│   └── 0002_extensions.sql
├── scripts/
│   └── dev.sh / dev.ps1
├── .editorconfig
├── .gitignore
├── biome.json
├── Cargo.toml                    # workspace root
├── deny.toml
├── LICENSE                       # Apache-2.0
├── LICENSES.md
├── package.json                  # pnpm root
├── pnpm-workspace.yaml
├── README.md
└── rust-toolchain.toml
```

## Repositories

**Un seul repository GitHub: `lochor/lochor`.** Pas de multi-repo.

- `main` = branche stable de dev.
- `release/*` branches de release.
- Tags `v0.1.0`, `v0.2.0`... pour les releases.
- GitHub Releases pour les binaires (CI build matrix).

## Conventions de nommage

| Type | Convention | Exemple |
| --- | --- | --- |
| Crate Rust (lib) | `lochor-<kebab>` | `lochor-storage`, `lochor-mcp` |
| Crate Rust (bin) | `lochor-<kebab>` (bin name idem) | `lochor-daemon`, `lochor-cli` |
| Package npm | `@lochor/<kebab>` | `@lochor/desktop`, `@lochor/ui-core` |
| Binaire distribué | `lochor` (CLI), `lochor-daemon`, `lochor-remote-server`, `lochor-supervisor` | — |
| Service systemd | `lochor-remote-server.service` | — |
| Port daemon | `7474` | `127.0.0.1:7474` |
| Port remote-server | `7473` | `0.0.0.0:7473` (TLS) |
| Config dir | `~/.lochor/` (global/user), `.lochor/` (workspace) | — |
| Plugin manifest | `plugin.json` | — |
| MCP config | `.lochor/mcp.json` (workspace), `~/.lochor/mcp.json` (global) | — |
| Rules | `.lochor/rules/*.md` + `LOCHOR.md` (workspace) | — |
| Env vars | `LOCHOR_*` | `LOCHOR_SERVER_URL`, `LOCHOR_TOKEN` |

## Responsabilités de chaque package

### `packages/` (Rust crates — le cœur)

| Crate | Responsabilité |
| --- | --- |
| `shared-types` | Types partagés (Session, Project, Message, Task, Artifact, Provider, Permission, ExtensionKind...) sérialisables serde. Zero dépendance métier. |
| `sdk` | Client HTTP/SSE du daemon et du remote-server. utilisé par CLI et (option) desktop. Réessaie, fallback, healthcheck. |
| `auth` | Token management, keychain, login/refresh/logout, Argon2id hash côté serveur, audit. |
| `config` | Chargement/merge config par scope (global/user/workspace), `~/.lochor/config.toml`, `.lochor/config.toml`, env vars. |
| `storage` | SQLite via sqlx, migrations, repositories (projects, sessions, messages, tasks, artifacts, extensions...). Abstraction pour future compat PostgreSQL. |
| `events` | Types d'événements (TokenStream, TaskUpdate, LogLine, PreviewUpdate, ProviderChanged, ExtensionEvent...) + bus local + sérialisation SSE. |
| `preview` | Modèle d'artefact, génération HTML sandboxed, CSP, export Python→HTML/PNG hooks. |
| `extensions` | Registry, loader par scope, manifest validation, permissions, hot-reload, import Claude Code/Cursor/Continue/Cline. |
| `mcp` | Wrapper rmcp, client + host MCP server, registre `.mcp.json`, transport stateless HTTP + stdio. |
| `plugin-sdk` | API pour auteurs de plugins (déclarer tools/hooks/skills/commands/agents/MCP/rules/LSP). Macros Rust + bindings TS. |
| `command-runtime` | Exécution slash commands + commands, résolution variables, tool gating. |
| `hook-runtime` | PreToolUse/PostToolUse/Stop/SessionStart/... events, exécution hooks shell avec timeout + permissions. |
| `skill-runtime` | Chargement SKILL.md, auto-trigger ou `/skill`, injection system prompt. |
| `agent-runtime` | Subagents spécialisés, isolation contexte, tool subset, model override, agent profiles. |
| `rules-runtime` | Agrégation `LOCHOR.md` + `.lochor/rules/*.md` par scope, injection system prompt, priorité global<workspace. |
| `lsp-adapters` | Adaptateurs LSP (towers-lsp ou wrapper), registration par projet, exposé comme tool agent. |

### `packages-ui/` (React/TS — partagé desktop)

| Package | Responsabilité |
| --- | --- |
| `core` | Design tokens, boutons, panels, layout primitives, theme. |
| `chat` | Chat panel, message rendering, tool-call approvals, streaming. |
| `preview` | Preview iframe sandboxed, markdown render, artifact switcher. |
| `terminal` | xterm.js wrapper, PTY via Tauri command. |

### `apps/`

| App | Responsabilité |
| --- | --- |
| `cli` | Thin client: parse args (clap), parle au daemon via `lochor-sdk`, affiche tokens/logs, `--no-daemon` embarque le core. |
| `desktop` | Tauri v2 shell: embarque core in-process, UI React/TS (4 panneaux), Monaco, xterm, preview, gestion extensions/MCP/rules dans l'UI. |

### `services/`

| Service | Responsabilité |
| --- | --- | 
| `daemon` | Daemon local loopback :7474, HTTP/SSE, gère sessions/projets/persistence, charge extensions, parle au provider-supervisor. |
| `remote-server` | Gateway sécurisée :7473, TLS, auth, sessions, audit, healthchecks, streaming, providers côté serveur. Module enterprise (BSL) pour collaboration/DGX/gate. |
| `provider-supervisor` | Auto-start/supervise Ollama/llama-server/LM Studio/vLLM sur loopback, healthchecks, idle shutdown. Sidecar du daemon ou binaire standalone. |

## Versioning

- **SemVer 2.0** pour tous les packages.
- Workspace version unifié `0.1.0` en MVP; chaque crate peut diverger à partir de `1.0.0`.
- `lochor-*` crates internes: version workspace jusqu'à 1.0, puis versionnage indépendant.
- Changelog par release (`CHANGELOG.md` racine).
- **Manifest plugin versioning**: `plugin.json` a `apiVersion` (Lochor extension API) + `version` (version du plugin). Lochor refuse les plugins dont `apiVersion` n'est pas supportée.

## CI/CD

### `ci.yml` (sur chaque PR + push main)

1. `cargo fmt -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cargo deny check`
5. `pnpm install --frozen-lockfile`
6. `pnpm typecheck` + `pnpm lint` (biome)
7. Build Tauri (dev mode, pas de bundle) pour vérifier la compil desktop.

### `build-matrix.yml` (sur tag + nightly)

Matrix:

| OS | Arch | Targets |
| --- | --- | --- |
| windows-2022 | x64 | `x86_64-pc-windows-msvc` |
| windows-2022-arm | arm64 | `aarch64-pc-windows-msvc` |
| macos-14 | arm64 | `aarch64-apple-darwin` |
| macos-13 | x64 | `x86_64-apple-darwin` |
| ubuntu-22.04 | x64 | `x86_64-unknown-linux-gnu` |
| ubuntu-22.04-arm | arm64 | `aarch64-unknown-linux-gnu` |

Produits:
- `lochor-cli-<target>` (bin)
- `lochor-daemon-<target>` (bin)
- `lochor-remote-server-<target>` (bin)
- `lochor-supervisor-<target>` (bin)
- `Lochor-<version>-<os>-<arch>.<pkg>` (Tauri bundle: MSI/NSIS sur Win, DMG sur macOS, AppImage/deb sur Linux)

### `release.yml` (sur tag `v*`)

1. Run `build-matrix`.
2. Upload artifacts sur GitHub Release.
3. Générer SHA256 + sigstore (cosign) pour les binaires.
4. Publier `CHANGELOG.md`.

## Release strategy

- **CalVer minor**: `v0.Y.Z` en pre-1.0 (iterations rapides), puis `v1.0.0` au MVP stable.
- **LTS**: pas d'LTS en V1; branches `release/X.Y` avec backports security only.
- **Releases fréquence**: pre-1.0 mensuelle, post-1.0 trimestrielle + hotfix.
- **Enterprise module BSL change date**: chaque tag enterprise a sa `Change Date` = release + 4 ans.
