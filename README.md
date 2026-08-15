# Locaryn

> Open-core agentic coding platform — native desktop + CLI + secured remote server, sharing one Rust core.

Locaryn is an open-source, LLM-assisted development platform built around a single
shared core that powers both a native desktop application (Tauri v2 + React/TS) and
a lightweight CLI. A secured remote server acts as an authenticated gateway to
distant providers, while a local daemon and provider supervisor handle local
runtimes (Ollama, llama.cpp, LM Studio, vLLM) on loopback only.

The platform is **extension-first**: MCP servers, plugins, slash commands, hooks,
skills, specialized agents, workspace rules, and LSP adapters are first-class
citizens with a unified manifest, scoped loading (global / user / workspace),
permission gating, and hot-reload. A compatibility layer can import bundles in
the spirit of Claude Code, Cursor, Continue.dev, Cline, and Antigravity.

## License (open-core)

| Component | License |
| --- | --- |
| `apps/*`, `services/daemon`, `services/provider-supervisor`, `packages/*`, `examples/*`, `migrations/*`, `docs/*` | **Apache-2.0** |
| `services/remote-server` core (auth, gateway, collaboration) | **Apache-2.0** |
| `services/remote-server/enterprise` (team context sharing, DGX Spark orchestration, concurrent-client gate) | **BSL 1.1** (converts to Apache-2.0 after 4 years) |

See [LICENSES.md](LICENSES.md) for the full breakdown and the rationale in
[docs/adr/ADR-0003-open-core-license.md](docs/adr/ADR-0003-open-core-license.md).

## Repository layout

```
locaryn/
├── apps/
│   ├── cli/                 # Rust CLI (clap) — thin client over the daemon
│   └── desktop/             # Tauri v2 + React + TS (Monaco, xterm.js, preview)
├── services/
│   ├── daemon/              # Local daemon (Rust) — loopback HTTP/SSE API
│   ├── remote-server/       # Secured remote gateway (Rust) — TLS, auth, audit
│   └── provider-supervisor/ # Auto-starts/supervises local LLM runtimes
├── packages/                # Shared Rust crates (the single core)
│   ├── shared-types/ sdk/ auth/ config/ storage/ events/
│   ├── preview/ extensions/ mcp/ plugin-sdk/
│   └── command-runtime/ hook-runtime/ skill-runtime/ agent-runtime/
│       rules-runtime/ lsp-adapters/
├── packages-ui/             # Shared React/TS UI (Tauri frontend reuse)
├── docs/
│   ├── architecture/        # Product spec, architecture, API, persistence, UX,
│   │                        #   roadmap, server mode & deployment
│   └── adr/                 # Architecture Decision Records
├── examples/                # Plugin/MCP/skill/command/agent/hooks/rules samples
├── migrations/              # SQLite migrations
└── .github/workflows/       # CI
```

## Quick start

```bash
# Rust toolchain (1.83+), Node 22+, pnpm 9+
pnpm install
cargo check --workspace
cargo build -p locaryn-daemon -p locaryn-cli

# Run the local daemon (loopback :7474)
cargo run -p locaryn-daemon

# In another shell — the agent, working in the current directory
cargo run -p locaryn-cli

# …or a plain conversation with no access to your files
cargo run -p locaryn-cli -- chat

# Desktop app (dev)
cd apps/desktop && pnpm tauri dev
```

### One-shot dev launcher (Windows)

For Windows, a batch launcher is provided in [`scripts/dev.bat`](scripts/dev.bat).
macOS and Linux users should follow the manual steps in Quick start above.
It builds the daemon and CLI, starts the daemon (or reuses an already-running
healthy one), then launches the Tauri desktop in dev mode. When the desktop dev
server exits, the launcher stops only the daemon it started.

```batch
scripts\dev.bat
```

The script automatically changes to the repository root, so it can be launched
from any directory.

> **Note:** closing the terminal window skips cleanup and leaves the daemon
> running. Stop it manually with `taskkill /IM locaryn-daemon.exe /F`.

## Running as a shared server

The desktop application and `locaryn-daemon` expose the same HTTP API, so a
machine with a GPU can serve a whole team. Security follows the listening
address rather than a setting:

| Listening on | Authentication | Encryption |
| --- | --- | --- |
| `127.0.0.1` | not required | none — traffic never leaves the machine |
| anything else | **required** | **required (TLS)** |

A server that is reachable but unprotected therefore cannot exist because a
checkbox was missed — the daemon refuses to start exposed with no account.

```bash
# 1. Create the first administrator (reads the password from stdin)
locaryn users add patron --admin

# 2. Expose the daemon. TLS is set up automatically on first start;
#    the certificate fingerprint is printed for clients to verify.
LOCARYN_DAEMON_BIND=0.0.0.0 locaryn-daemon

# 3. Produce the settings employees will need
locaryn provision 192.168.1.188 --org "Your Company"
```

Step 3 writes `locaryn-connect.json`. Put it next to the `.msi` or in
`C:\ProgramData\Locaryn\`, and staff only have to install the app and type the
credentials they were given — no address, no port, no certificate to copy. The
file holds no secret and can be distributed freely.

Full details, and the reasoning behind each choice, in
[docs/architecture/12-server-and-deployment.md](docs/architecture/12-server-and-deployment.md).

## Installation et mises à jour

Les binaires sont publiés sur la page **Releases** de GitHub :
<https://github.com/Locaryn/locaryn/releases>.

### Quel fichier télécharger ?

Application de bureau — elle embarque le service Locaryn et la commande `locaryn`,
donc elle sait aussi servir le réseau local et se piloter depuis un terminal :

| Plateforme | Fichier | Remarque |
| --- | --- | --- |
| Windows x64 | `Locaryn_<version>_x64-setup.exe` | installeur NSIS — recommandé |
| Windows x64 | `locaryn-<version>-desktop-windows-x64-portable.zip` | portable, sans auto-update |
| macOS | `Locaryn_<version>_universal.dmg` | universel — Intel et Apple Silicon |
| Linux x64 | `Locaryn_<version>_amd64.deb` | Debian / Ubuntu |
| Linux x64 | `Locaryn_<version>_amd64.AppImage` | portable, sans installation |
| Linux x64 | `locaryn-<version>-desktop-linux-x64-portable.tar.gz` | portable, sans auto-update |
| Android | `locaryn-<version>-android.apk` | signé si le dépôt fournit un keystore ; sinon publié `-unsigned` et à signer soi-même (`scripts/android-keystore.ps1`) |

Serveur seul, pour une machine sans session graphique — `locaryn-daemon` + la CLI :

| Plateforme | Fichier | Remarque |
| --- | --- | --- |
| Windows x64 | `locaryn-<version>-server-windows-x64.msi` | installe le service et la CLI, les ajoute au PATH — à lancer en administrateur |
| Windows x64 | `locaryn-<version>-server-windows-x64.zip` | portable |
| Linux x64 | `locaryn-<version>-server-linux-x64.deb` | Debian / Ubuntu — incompatible avec le paquet de bureau, qui contient déjà le service |
| Linux x64 · macOS | `locaryn-<version>-server-<plateforme>.tar.gz` | portable |

Règle simple : prenez l'**installeur** de votre plateforme. L'installeur reçoit les
**mises à jour automatiques** ; une archive portable doit être retéléchargée à la main.
Les noms `Locaryn_<version>_<arch>…` sont la convention attendue par le système de mise
à jour — ne les renommez pas.

### Depuis un terminal

```bash
locaryn daemon start   # démarre le service en arrière-plan, attend qu'il réponde
locaryn status         # mode, fournisseur actif, version
locaryn                # l'agent, dans le dossier courant
locaryn daemon logs    # ce que le service a écrit
locaryn daemon stop
```

### Mises à jour automatiques

Windows et macOS embarquent un client de mise à jour qui interroge la release la plus
récente sur GitHub (`Réglages → À propos → Vérifier les mises à jour`). La vérification
se fait aussi au démarrage. Sous Linux, l'updater Tauri n'est pas supporté : il faut
télécharger le nouveau paquet depuis la page Releases.

Pour que ce mécanisme fonctionne, le dépôt doit être **public** (c'est le cas) et les
releases doivent être **publiées** avec leurs installeurs signés. Les artefacts macOS ne
sont pas notarisés : la première ouverture peut demander une confirmation manuelle.

## Status

MVP target: 6–8 weeks. See [docs/architecture/10-roadmap.md](docs/architecture/10-roadmap.md).
