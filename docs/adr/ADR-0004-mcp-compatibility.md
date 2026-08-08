# ADR-0004 — MCP standard compatibility + Lochor extension packaging

## Context
Lochor must be extension-first and compatible with the modern agentic ecosystem (Claude Code, Cursor, Continue, Cline, Antigravity). MCP is the de facto standard for tool servers as of mid-2026 (spec 2026-07-28: stateless HTTP + stdio). Claude Code/Cursor/Cline also use markdown-based rules, slash commands, agent frontmatter, and skills — but none define a unified, permissioned, scoped bundle format.

## Decision
- **MCP:** implement the standard spec via `lochor-mcp` (rmcp wrapper). Support stateless HTTP + stdio transports; `server/discover`; tools/resources/prompts/tasks; JSON Schema 2020-12. Register MCP servers per scope (global/user/workspace) via `.lochor/mcp.json` using the de facto `mcpServers: {name: {command, args, env}}` format (compatible with Claude Code/Cursor).
- **Deprecated MCP features** (Roots, Sampling, Logging): map to Lochor equivalents when useful (Roots → workspace rules; Sampling → direct provider API; Logging → OpenTelemetry). Not reimplemented as-is.
- **Lochor extension packaging:** a Lochor plugin is a bundle with `plugin.json` manifest declaring `apiVersion`, `permissions`, `components` (skills/commands/agents/hooks/mcp/rules/lsp), `config.schema`, `deps`. Scoped (global/user/workspace/session), permission-gated, hot-reloadable.
- **Import layer:** `lochor-extensions` provides `lochor import claude-code|cursor|continue|cline` converters. Markdown-based concepts (rules, slash commands, agents, skills) import directly; YAML manifests (Continue, Antigravity) and Claude Code `plugin.json` require a light adapter; Lochor's permission/scope/packaging layer is specific to Lochor.

## Consequences
- **Positive:** Direct compatibility with `.mcp.json` and markdown conventions → low friction for users coming from Claude Code/Cursor; Lochor's value-add is the permissioned, scoped, hot-reloadable bundle + sandbox (beyond what MCP or markdown conventions define).
- **Negative:** Must track MCP spec evolution (2026-07-28 is a breaking transition from the session-based model); `rmcp` maturity risk mitigated by an abstraction wrapper.
- **Neutral:** Lochor plugin format is not a standard — it's our packaging layer. We don't propose it as a competitor to MCP; it encapsulates MCP servers.

## Alternatives considered
- **Pure MCP, no Lochor plugin format:** rejected — MCP doesn't define permissions, scoping, packaging, or non-tool extensions (skills/hooks/agents/rules). We need a layer above.
- **Clone Claude Code plugin format exactly:** rejected — proprietary; we want conceptual compatibility, not a copy. Lochor adds permissions + scope + sandbox.
- **Adopt Continue's `config.yaml` as our format:** rejected — YAML, no permissions model, less structured.

## References
- `docs/architecture/03-tech-decisions.md` (D12, D13)
- `docs/architecture/09-extension-model.md`
