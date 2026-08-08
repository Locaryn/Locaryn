# 09 — Extension Model

Système d'extensions **first-class**. Un plugin Locaryn est un bundle auto-contained pouvant contenir: skills, commands/slash commands, hooks, agents, MCP servers, rules, LSP adapters. Manifest `plugin.json`, permissions déclarées, scoping, hot-reload.

## Structure d'un plugin Locaryn

```
my-plugin/
├── plugin.json                 # manifest (obligatoire)
├── README.md
├── skills/
│   └── database-migration/
│       └── SKILL.md
├── commands/
│   ├── refactor.md             # slash command /refactor
│   └── build.md
├── agents/
│   └── code-reviewer.md
├── hooks/
│   └── hooks.json
├── mcp/
│   └── mcp.json                # déclare les MCP servers du plugin
├── rules/
│   └── security.md             # règles workspace apportées par le plugin
├── lsp/
│   └── lsp.json
└── src/                        # code natif optionnel (Rust WASM ou TS)
    └── ...
```

## Manifest `plugin.json`

```json
{
  "schema": "https://locaryn.dev/schema/plugin.json/v0.1",
  "apiVersion": "0.1",
  "name": "my-plugin",
  "version": "1.0.0",
  "description": "Code review and database migration helpers",
  "author": "Jane Doe <jane@example.com>",
  "license": "Apache-2.0",
  "homepage": "https://github.com/jane/my-plugin",
  "repository": "https://github.com/jane/my-plugin",
  "keywords": ["review", "migration", "sql"],
  "minLocarynVersion": "0.1.0",
  "permissions": {
    "shell": { "reason": "Run migrations and linters", "scope": "project" },
    "files.read": { "reason": "Read source files for review", "scope": "project" },
    "files.write": { "reason": "Apply suggested refactors", "scope": "project", "requireApproval": true },
    "network": false,
    "mcp": { "reason": "Expose a custom MCP server for schema introspection", "scope": "project" },
    "lsp": false,
    "extensions": false,
    "env": ["DB_URL"]
  },
  "components": {
    "skills": ["skills/database-migration/SKILL.md"],
    "commands": ["commands/refactor.md", "commands/build.md"],
    "agents": ["agents/code-reviewer.md"],
    "hooks": "hooks/hooks.json",
    "mcp": "mcp/mcp.json",
    "rules": ["rules/security.md"],
    "lsp": "lsp/lsp.json"
  },
  "config": {
    "schema": {
      "strict_mode": { "type": "boolean", "default": false }
    }
  }
}
```

### Champs du manifest

| Champ | Rôle |
| --- | --- |
| `schema` | URL du schema JSON de validation (versionné) |
| `apiVersion` | Version de l'extension API Locaryn supportée |
| `name`, `version`, `description`, `author`, `license` | Métadonnées |
| `minLocarynVersion` | Version Locaryn minimum requise |
| `permissions` | Permissions demandées (voir §Permissions) |
| `components` | Chemins (relatifs à la racine du plugin) vers les sous-éléments |
| `config.schema` | JSON Schema des options de configuration du plugin (exposées au user) |

## Scopes

| Scope | Dossier | Visibilité |
| --- | --- | --- |
| `global` | `~/.locaryn/plugins/` | Tous les projets de l'utilisateur |
| `user` | `~/.locaryn/plugins/` (alias de global en V1) | Identique |
| `workspace` | `<project>/.locaryn/plugins/` | Le projet uniquement |
| `session` | transitoire (non persisté) | La session courante seulement |

Résolution: `workspace` > `user` > `global` (le plus spécifique gagne pour un même `name`).

## Lifecycle d'installation

```
1. install(source, scope)
   ├── resolve source (path | url | registry)
   ├── download/extract vers <scope dir>/<name>/
   ├── validate plugin.json (schema + apiVersion + minLocarynVersion)
   ├── check dependencies (déclarées dans plugin.json deps[])
   ├── persist extension row (status=installing)
   └── prompt permissions → user decision
2. enable(id)
   ├── load components (skills, commands, agents, hooks, mcp, rules, lsp)
   ├── register in runtime registries
   ├── start MCP servers (auto_start ones)
   └── status=installed, enabled=1
3. (hot-reload) on fs change
   ├── diff manifest + components
   ├── re-validate
   ├── re-register (atomic swap)
   └── emit ExtensionEvent(reloaded)
4. disable(id)
   ├── unregister components
   ├── stop MCP servers
   └── enabled=0
5. remove(id)
   ├── disable
   ├── delete row + files
   └── emit ExtensionEvent(removed)
```

## Dépendances

`plugin.json` peut déclarer:
```json
"deps": [
  { "name": "locaryn-mcp-stdlib", "version": "^1.0.0" },
  { "name": "another-plugin", "version": ">=2.0.0" }
]
```
Résolution: registry local; si manquant, refus d'install avec message clair. Pas de résolution recursive complexe en V1 (flat deps).

## Chargement runtime

- **Skills/commands/agents/rules:** markdown parsed (YAML frontmatter + body), injectés dans les registries correspondants (`locaryn-skill-runtime`, etc.).
- **Hooks:** `hooks.json` parsé, enregistrés dans `locaryn-hook-runtime`.
- **MCP servers:** `mcp/mcp.json` parsé, enregistrés dans `locaryn-mcp` registry; démarrés selon `auto_start`.
- **LSP:** `lsp/lsp.json` parsé, enregistrés dans `locaryn-lsp-adapters`.
- **Code natif (optionnel V1.1):** `src/` compilé en WASM (sandbox `wasmtime`) ou bindings TS via `locaryn-plugin-sdk`. V1: markdown + MCP only; code natif repoussé en V1.1.

## Permissions

Déclarées dans `plugin.json.permissions`. Approuvées à l'install (modal desktop / prompt CLI). Refus = feature désactivée, agent informé dans system prompt.

| Permission | Description | Default |
| --- | --- | --- |
| `shell` | Exécuter commandes | refusé |
| `files.read` | Lire fichiers workspace | accordé si project trusted |
| `files.write` | Écrire fichiers workspace | refusé (requireApproval) |
| `network` | fetch / MCP HTTP | refusé |
| `mcp` | Enregistrer/activer MCP servers | refusé |
| `extensions` | Gérer autres extensions | refusé |
| `preview` | Ouvrir artefacts en preview | accordé |
| `lsp` | Enregistrer LSP | refusé |
| `env` | Lire vars d'env (liste explicite) | refusé |

Format:
```json
"shell": { "reason": "...", "scope": "once|session|project|always", "requireApproval": false }
```
ou `false` pour explicitement ne pas demander.

## Sandbox

- **Markdown components** (skills/commands/agents/rules): pas d'exécution de code; injection system prompt uniquement. Safe par construction.
- **Hooks:** exécutés via shell avec timeout + permission `shell` + working dir = project root; stdout/stderr capturés; variables `${LOCARYN_PLUGIN_ROOT}`, `${LOCARYN_PROJECT_ROOT}`, `${LOCARYN_SESSION_ID}` injectées.
- **MCP servers:** exécutés en subprocess (stdio) ou contactés via HTTP; permissions `mcp` + `network` (si HTTP); outils MCP soumis à approval comme les outils natifs.
- **Code natif (V1.1):** WASM `wasmtime` sandbox, pas d'accès FS/réseau direct; IPC via host functions permission-gated.
- **Preview:** iframe sandboxed + CSP; pas d'accès au app origin.

## Support des composants

### Hooks (`hooks/hooks.json`)

Format compatible Claude Code:
```json
{
  "PreToolUse": [
    {
      "matcher": "WriteFile",
      "hooks": [
        { "type": "command", "command": "bash ${LOCARYN_PLUGIN_ROOT}/hooks/validate.sh", "timeout": 30 }
      ]
    }
  ],
  "PostToolUse": [
    { "matcher": "*", "hooks": [{ "type": "command", "command": "echo done" }] }
  ],
  "Stop": [{ "hooks": [{ "type": "command", "command": "notify-send 'Locaryn done'" }] }]
}
```
Events: `PreToolUse`, `PostToolUse`, `Stop`, `SubagentStop`, `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreCompact`, `Notification` (même vocabulaire que Claude Code pour compat).

### Skills (`skills/<name>/SKILL.md`)

```markdown
---
name: database-migration
description: When the user wants to run or create SQL migrations.
version: 1.0.0
auto_trigger: true
allowed_tools: [read_file, write_file, run_command]
---

# Instructions

Analyze the schema changes, check the `migrations/` directory, verify SQL syntax
before executing against the target DB.
```

### Commands / slash commands (`commands/refactor.md`)

```markdown
---
name: refactor
description: Refactor code extract module
allowed_tools: [read_file, write_file, search]
arguments: ["operation", "target"]
---

Extract `$1` from `$2` into a dedicated module. Read the file, identify the
construct, create the new module, update imports.
```
Invocation: `/refactor extract-module src/auth.ts`.

### Agents (`agents/code-reviewer.md`)

```markdown
---
name: code-reviewer
description: Expert in security and performance code reviews.
model: qwen2.5-coder:32b
tools: [read_file, search]
output_style: concise
---

You are a senior staff engineer. Prioritize identifying security vulnerabilities
and performance bottlenecks in the diffs provided.
```

### MCP (`mcp/mcp.json`)

Format compatible Claude Code/Cursor (`mcpServers`):
```json
{
  "mcpServers": {
    "schema-introspect": {
      "command": "node",
      "args": ["${LOCARYN_PLUGIN_ROOT}/mcp/schema-server.js"],
      "env": { "DB_URL": "${env:DB_URL}" },
      "transport": "stdio",
      "auto_start": true
    },
    "remote-weather": {
      "url": "https://mcp.example.com/weather",
      "transport": "http",
      "headers": { "Authorization": "Bearer ${env:WEATHER_TOKEN}" }
    }
  }
}
```

### Workspace rules (`rules/security.md`)

```markdown
---
name: security
priority: 10
---

# Security rules for this project

- Never commit secrets. Scan staged files with gitleaks before commit.
- All new endpoints must require auth.
- Use parameterized queries only.
```
Agrégé avec `LOCARYN.md` et `.locaryn/rules/*.md` par `locaryn-rules-runtime`.

### LSP (`lsp/lsp.json`)

```json
{
  "adapters": [
    { "language": "rust", "command": "rust-analyzer", "args": [] },
    { "language": "typescript", "command": "typescript-language-server", "args": ["--stdio"] }
  ]
}
```

## Compatibilité écosystème — table de mapping

| Source | Concept | Format source | Mapping Locaryn | Compat |
| --- | --- | --- | --- | --- |
| Claude Code | `.claude/agents/*.md` | YAML frontmatter | `agents/*.md` (idem) | ✅ direct |
| Claude Code | `.claude/commands/*.md` | YAML + $0,$1 | `commands/*.md` | ✅ direct |
| Claude Code | `.claude/skills/*/SKILL.md` | YAML frontmatter | `skills/*/SKILL.md` | ✅ direct |
| Claude Code | `hooks.json` | events Claude | `hooks/hooks.json` | ✅ direct (mêmes events) |
| Claude Code | `output-styles/*.md` | markdown | `agents/*.md` output_style | ✅ adaptateur |
| Claude Code | `CLAUDE.md`, `rules/*.md` | markdown | `rules/*.md` + `LOCARYN.md` | ✅ direct |
| Claude Code | `plugin.json` (si présent) | manifest | `plugin.json` (conversion) | adaptateur (permissions à déclarer) |
| Cursor | `.cursor/mcp.json` | `mcpServers` | `mcp/mcp.json` | ✅ direct |
| Cursor | `.cursor/rules/*.md` | markdown | `rules/*.md` | ✅ direct |
| Continue | `config.yaml` models | YAML | `providers` config | adaptateur (YAML→TOML) |
| Continue | `config.yaml` mcpServers | YAML | `mcp/mcp.json` | adaptateur |
| Continue | `config.yaml` prompts | YAML | `commands/*.md` | adaptateur |
| Cline/Roo | `AGENTS.md` | markdown | `LOCARYN.md` | ✅ direct |
| Cline/Roo | `.roo/rules-*/*.md` | markdown dir | `rules/*.md` | ✅ direct |
| Cline/Roo | modes (UI) | — | `agent_profiles` | adaptateur |
| Antigravity | `antigravity.yaml` | YAML | `agent_profiles` + permissions | adaptateur |

### Compatible directement (aucune conversion)

- MCP `.mcp.json` / `mcpServers` (format standard de facto).
- Markdown rules / slash commands / agents (frontmatter YAML).
- `LOCARYN.md` ≡ `CLAUDE.md` ≡ `AGENTS.md` (fichier d'instructions racine).

### Nécessite un adaptateur

- Continue `config.yaml` (YAML → Locaryn JSON/TOML).
- Claude Code `plugin.json` (ajout des permissions Locaryn).
- Antigravity `antigravity.yaml` (persona → agent_profile).
- Cline "modes" (UI state → agent_profile).

### Reste spécifique Locaryn

- Manifest `plugin.json` avec `apiVersion`, `permissions` (modèle de sécurité), `config.schema`, packaging + scope.
- Permissions model (shell/files/network/... avec approval scope).
- Sandbox WASM (V1.1) pour code natif.
- Registry local + hot-reload.

## Commande d'import

```bash
locaryn import claude-code ./path/to/.claude
# → scan agents/, commands/, skills/, hooks.json, output-styles/, CLAUDE.md, rules/
# → convertit en structure Locaryn sous .locaryn/ (ou ~/.locaryn/plugins/imported-cc/)
# → résumé: "Imported 3 agents, 5 commands, 2 skills, 1 hooks file, 1 rules file"

locaryn import cursor ./path/to/.cursor
# → .cursor/mcp.json → .locaryn/mcp.json; .cursor/rules/*.md → .locaryn/rules/

locaryn import continue ./config.yaml
# → models → providers config; mcpServers → mcp.json; prompts → commands/

locaryn import cline ./path/to/project
# → AGENTS.md → LOCARYN.md; .roo/rules-*/*.md → rules/
```

## Exemples concrets (cf. `examples/`)

- `examples/plugins/my-plugin/` — plugin Locaryn complet (manifest + skill + command + agent + hooks + mcp + rules + lsp).
- `examples/mcp.json` — configuration MCP standalone.
- `examples/SKILL.md`, `examples/command.md`, `examples/agent.md`, `examples/hooks.json`, `examples/workspace-rules.md` — exemples unitaires.

---

## Serveurs MCP : où ils tournent vraiment

Le client MCP (`packages/mcp`) parle les deux transports : commande locale
(stdio) et HTTP. Le format de configuration est celui de Claude Code et de
Cursor — `mcpServers` dans `mcp.json` — donc un serveur écrit pour eux
fonctionne sans adaptation.

Le fichier est **partagé** : un serveur ajouté depuis l'application est visible
depuis le terminal, et inversement.

```bash
locaryn mcp add graphify "uvx graphify-mcp --graph mon-projet"
locaryn mcp test graphify     # le lance une fois et liste ses outils
locaryn mcp list
```

`test` ne demande pas de daemon : c'est ce qu'on lance *avant* de démarrer quoi
que ce soit. `start` et `stop`, si — un serveur est un processus enfant, et une
commande qui rend la main l'emporterait avec elle.

Les outils découverts arrivent au modèle sous le nom `mcp__<serveur>__<outil>`.

### Deux pièges corrigés en chemin

- **`npx` sur Windows** est `npx.cmd`, et `CreateProcess` n'applique pas
  `PATHEXT` : tout serveur npm échouait avec « program not found » sur une
  machine où `npx` marche pourtant dans un terminal. La résolution passe
  désormais par `PATH` + `PATHEXT`.
- **Les variables d'environnement** d'une entrée remplaçaient l'environnement
  entier, `PATH` compris — donc dès qu'une configuration précisait une clé
  d'API, le serveur ne trouvait plus l'interpréteur qui le lançait. Elles
  s'ajoutent maintenant.
