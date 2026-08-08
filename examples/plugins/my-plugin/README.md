# my-plugin

Example Lochor plugin demonstrating every component kind:

- `skills/database-migration/SKILL.md` — a skill
- `commands/refactor.md` — a slash command (`/refactor`)
- `agents/code-reviewer.md` — a specialized subagent
- `hooks/hooks.json` + `hooks/validate.sh` — PreToolUse/PostToolUse hooks
- `mcp/mcp.json` + `mcp/schema-server.js` — an MCP server
- `rules/security.md` — workspace rules
- `lsp/lsp.json` — LSP adapters

Install:

```bash
lochor plugin install ./examples/plugins/my-plugin --scope user
```
