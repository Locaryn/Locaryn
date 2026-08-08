# 02 — Architecture

Deux architectures étudiées. **A** est recommandée (cf. `03-tech-decisions.md`).

## Architecture A — Monorepo Rust-core, daemon in-process ou standalone, Tauri v2 (recommandée)

### Diagramme textuel

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                         Locaryn monorepo (Rust core)                          │
│                                                                              │
│   packages/* (16 crates) = LE CŒUR PARTAGÉ                                   │
│   shared-types sdk auth config storage events preview extensions             │
│   mcp plugin-sdk command-runtime hook-runtime skill-runtime                  │
│   agent-runtime rules-runtime lsp-adapters                                   │
└──────────────────────────────────────────────────────────────────────────────┘
        ▲                        ▲                          ▲
        │ in-process (lib)       │ standalone binary        │ standalone binary
        │                        │                          │
┌───────┴──────────┐    ┌────────┴─────────┐      ┌─────────┴──────────────┐
│ apps/desktop     │    │ services/daemon  │      │ services/remote-server │
│ Tauri v2 shell   │    │ loopback HTTP/SSE│      │ TLS HTTP/SSE gateway   │
│ ┌─────────────┐  │    │ :7474            │      │ :7473                  │
│ │ Rust core   │──┼────│ same crates      │      │ same crates + auth/    │
│ │ (embedded)  │  │    │ (server mode)    │      │ audit/enterprise       │
│ ├─────────────┤  │    └────────┬─────────┘      └─────────┬──────────────┘
│ │ React/TS UI │  │             │ loopback                  │ providers
│ │ Monaco      │  │             │                           │ configured
│ │ xterm.js    │  │             │                           │ server-side
│ │ preview     │  │             ▼                           ▼
│ └─────────────┘  │    ┌──────────────────┐        ┌────────────────────┐
└──────────────────┘    │ provider-        │        │ remote LLM         │
        ▲               │ supervisor       │        │ (OpenAI-compat,    │
        │ Tauri IPC     │ (sidecar/child)  │        │  DGX Spark, ...)   │
        │ commands/     │ → Ollama         │        └────────────────────┘
        │ events/       │ → llama-server   │
        │ channels      │ → LM Studio      │
                        │ → vLLM           │
┌───────┴──────────┐    │ all on loopback  │
│ apps/cli         │────┘                  │
│ clap, thin client│                       │
│ talks to daemon  │                       │
│ via locaryn-sdk   │                       │
└──────────────────┘                       │
                                           │
        Connexion client (3 modes):        │
        ┌──────────────────────────┐       │
        │ auto: remote→local fallback│     │
        │ remote: remote-server only │     │
        │ local: daemon only         │     │
        └──────────────────────────┘       │
```

### Composants

| Composant | Rôle | Techno |
| --- | --- | --- |
| `packages/*` (16 crates) | Cœur métier partagé: types, SDK client, auth, config, storage (SQLite), events, preview, extensions, MCP, plugin-sdk, runtimes (command/hook/skill/agent/rules), LSP adapters | Rust |
| `apps/desktop` | Native desktop: embarque le core en in-process (lib) **et** peut parler au daemon standalone. UI React/TS, Monaco, xterm.js, preview iframe sandboxed | Tauri v2 + React/TS |
| `apps/cli` | CLI légère, thin client: parle au daemon via `locaryn-sdk` (HTTP/SSE), ou en `--no-daemon` embarque le core | Rust + clap |
| `services/daemon` | Daemon local loopback :7474, HTTP/SSE, gère sessions/projets/persistence, supervise extensions, parle au provider-supervisor | Rust + axum |
| `services/remote-server` | Gateway sécurisée: TLS, auth, sessions, audit, healthchecks, streaming, providers configurés côté serveur, module enterprise (BSL) | Rust + axum + rustls |
| `services/provider-supervisor` | Auto-start/supervise les runtimes locaux sur loopback, healthchecks, idle shutdown | Rust + tokio::process |

### Flux de données

1. **Desktop local:** UI → Tauri command → core (in-process) → provider-supervisor → Ollama (loopback) → stream tokens → Tauri channel → UI.
2. **CLI local:** CLI → `locaryn-sdk` → HTTP/SSE → daemon → core → provider-supervisor → Ollama → SSE → CLI.
3. **Desktop/CLI remote (auto):** client → healthcheck remote-server → si OK: TLS + token → remote-server → provider distant (ou DGX) → SSE → client. Si KO: fallback daemon local.
4. **Extensions:** core charge plugin depuis `~/.locaryn/plugins/` ou `.locaryn/plugins/` → valide manifest → enregistre tools/hooks/skills/commands/agents/MCP → permissions prompt → hot-reload via fs watcher.
5. **Preview:** agent émet artifact → core écrit dans workspace artifacts → desktop preview panel charge l'artifact en iframe sandboxed (CSP strict, pas de network sauf permission).

### Choix techno (résumé — détail en `03`)

- Desktop: **Tauri v2** (React/TS). Sidecars supportés, IPC commands/events/channels, build x64+ARM64.
- Core/daemon/remote-server: **Rust** (partage natif avec Tauri).
- IPC local: **HTTP/SSE** (standard, debuggable, même contrat que remote).
- IPC desktop↔core: **Tauri commands/events/channels** (in-process, zéro réseau).
- Persistence: **SQLite** (sqlx).
- LLM providers: OpenAI-compat + Ollama natif.
- MCP: **rmcp** (Rust SDK), spec 2026-07-28 (stateless HTTP + stdio).

### Avantages

- **Zéro duplication** du cœur: desktop, CLI, daemon, remote-server utilisent les mêmes crates.
- **Faible latence desktop**: core in-process (pas de réseau pour la UI).
- **Single binary CLI/daemon**: distribution simple.
- **Même contrat API** local (daemon) et remote (remote-server): le SDK client est identique, seul l'endpoint change.
- **Hot-reload extensions** via fs watcher + registry.
- **ARM64 natif** via Rust cross-compilation + Tauri.

### Inconvénients

- Courbe d'apprentissage Rust (si l'équipe n'est pas familière).
- Tauri v2 webview: preview HTML sandboxed OK, mais pas un Chromium full (Edge WebView2 sur Windows, WebKit ailleurs) — tester la compatibilité des artefacts complexes.
- Moins de libs Python pour la data viz (preview de graphiques Python → exporter en HTML/PNG côté runtime, pas natif).

### Risques

| Risque | Mitigation |
| --- | --- |
| Dépendance webview système (différences Edge/WebKit) | Preview standardisée sur HTML/CSS/JS portable; tests cross-webview |
| Sidecar provider-supervisor cross-arch | Build matrix CI; fallback "user-provided Ollama" si binaire absent |
| `rmcp` maturité vs Python SDK | Wrapper d'abstraction dans `locaryn-mcp`; adapter si besoin |
| Remote-server enterprise BSL adoption | Change date 4 ans + gate fonctionnel, pas juridique |

## Architecture B — Electron + Python daemon + Go remote-server (alternative)

### Diagramme textuel

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  apps/desktop (Electron + React)     services/daemon (Python/FastAPI)        │
│  ┌────────────────────────────┐      ┌────────────────────────────────┐     │
│  │ Main process (Node)        │◀────▶│ FastAPI + uvicorn              │     │
│  │  └ spawn python daemon     │ HTTP │ SQLite via SQLAlchemy          │     │
│  │ React renderer (Chromium)  │ /SSE │ MCP Python SDK                 │     │
│  │ Monaco, xterm.js, preview  │      │ provider-supervisor (asyncio)  │     │
│  └────────────────────────────┘      └────────────────────────────────┘     │
│                                                                              │
│  apps/cli (Python/Click) ──HTTP──▶ daemon                                    │
│                                                                              │
│  services/remote-server (Go + Gin)  ──TLS──▶ clients                         │
│   - auth, audit, gateway                                                     │
│   - ne partage PAS le code Python du daemon (duplication logique)            │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Composants

- Desktop: **Electron** (Chromium full + Node main) + React/TS.
- Daemon + CLI: **Python** (FastAPI + Click), SQLite via SQLAlchemy, MCP Python SDK.
- Remote-server: **Go** (Gin), TLS, auth, audit, gateway.
- Provider-supervisor: asyncio subprocess dans le daemon.

### Avantages

- Python: écosystème data viz riche (matplotlib/plotly) pour preview de graphiques.
- Electron: Chromium full, preview d'artefacts complexes 100% compatible.
- Go remote-server: binaire statique simple, excellent pour un service réseau.

### Inconvénients (rédhibitoires)

- **Duplication du cœur**: daemon Python et remote-server Go ne partagent pas de logique — chaque feature agentique doit être écrite 2×.
- **CLI Python**: distribution lourde (PyInstaller/Nuitka) ou dépendance runtime Python sur le poste utilisateur.
- **Electron**: binaire 150–200MB, RAM élevée, contraire à l'objectif natif léger.
- **3 langages** (TS + Python + Go): complexité de monorepo, CI, et recrutement.
- Pas de partage natif avec Tauri (Rust) — l'argument "même cœur" s'effondre.

### Risques

| Risque | Mitigation |
| --- | --- |
| Duplication logique daemon/remote | Protocole strict + double maintenance (coût élevé) |
| Distribution CLI Python | Sidecar Python embarqué (lourd) |
| Electron taille/RAM | Acceptable pour desktop, mais contraire au brief |

### Verdict

**Architecture A retenue.** Le brief exige explicitement "le même cœur métier" entre desktop, CLI, daemon et remote-server. Seul Rust permet ce partage natif (avec Tauri). L'architecture B casse cette exigence fondamentale. Voir `03-tech-decisions.md`.
