# ADR-0002 — HTTP/1.1 + SSE transport

## Context
Locaryn needs a client-facing API for the local daemon (loopback) and the remote-server (TLS). Both surfaces (desktop, CLI) must speak the same contract. Streaming is required for tokens, logs, task state, preview updates — all server→client. Rare bidirectional needs: preview live-reload, interactive shell.

## Decision
- **HTTP/1.1 + SSE** as the primary transport for both daemon and remote-server.
- Streaming via Server-Sent Events (`text/event-stream`), typed `event:` lines.
- Bidirectional cases use **POST + SSE response** (e.g., send a message, stream the reply) or a Tauri `Channel` for the desktop in-process path.
- MCP transport is handled separately by `locaryn-mcp` per the MCP 2026-07-28 spec (stateless HTTP + stdio) — not part of this decision.

## Consequences
- **Positive:** Debuggable with `curl`; native browser/Tauri `EventSource`; no protobuf/grpc-web toolchain; identical contract local and remote (only endpoint + TLS differ); simple CDN/proxy-compatible.
- **Negative:** SSE is server→client only; bidirectional requires POST+SSE or a second channel (acceptable for our usage).
- **Neutral:** No binary framing — JSON payloads. Token streaming volume is modest; JSON is fine.

## Alternatives considered
- **gRPC:** rejected — requires protobuf + grpc-web for the Tauri frontend; tooling overhead; marginal gain for our streaming pattern.
- **WebSocket:** rejected — bidirectional but overkill; SSE covers 95% of needs (server→client); WebSocket adds connection management complexity.

## References
- `docs/architecture/03-tech-decisions.md` (D4)
- `docs/architecture/06-api-contract.md`
