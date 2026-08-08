# ADR-0001 — Rust core + Tauri v2 desktop

## Context
Lochor requires desktop + CLI + local daemon + remote-server to share **exactly the same** business core (no agent logic duplication in the UI). The desktop must be native, lightweight, multi-arch (x64 + ARM64) on Windows/macOS/Linux, with an integrated preview panel and sidecar support for local LLM runtimes.

## Decision
- Core: **Rust** crates workspace (`packages/*`) — the single shared core.
- Daemon, remote-server, provider-supervisor: Rust binaries reusing the core.
- CLI: Rust + clap, thin client over the daemon (or in-process core via `--no-daemon`).
- Desktop: **Tauri v2** + React/TS. The Tauri Rust shell embeds the core in-process; IPC via Tauri commands/events/channels. Sidecars for provider-supervisor.
- Frontend: Monaco (light edits), xterm.js (terminal), sandboxed iframe (preview).

## Consequences
- **Positive:** Zero core duplication; single-binary CLI/daemon; native lightweight desktop (~15MB shell); first-class ARM64 via cargo + tauri-action; memory-safe permission gating; streaming via tokio + Tauri channels.
- **Negative:** Rust learning curve if the team is not fluent; webview differences (Edge WebView2 vs WebKit) for exotic preview artifacts; fewer data-viz libs vs Python (mitigated by exporting Python plots to HTML/PNG).
- **Neutral:** MCP SDK is `rmcp` (Rust) — mature in 2026 but slightly younger ecosystem than Python's.

## Alternatives considered
- **Electron + Python daemon + Go remote-server (Arch B):** rejected — duplicates agent logic across Python (daemon) and Go (remote-server); Electron is 150-200MB; CLI Python distribution is heavy. Violates the "same core" hard requirement.
- **Rust core + Electron desktop:** rejected — loses in-process embedding (would need FFI/subprocess); Electron size/RAM.
- **Go daemon + Tauri:** rejected — no native core sharing with Tauri (Rust); FFI overhead.

## References
- `docs/architecture/02-architecture.md`
- `docs/architecture/03-tech-decisions.md` (D1, D2, D3)
