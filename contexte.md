# Lochor — Contexte Produit

> Document de référence rassemblant la vision fonctionnelle, l'expérience utilisateur, le design system et les choix techniques du projet Lochor.

---

## 1. Vue d'ensemble

**Lochor** est une application de développement assisté par LLM, open-source, modulaire et orientée code. Elle fonctionne à la fois en :

- **Application desktop native** (Windows / macOS / Linux)
- **CLI**
- **Daemon local** partagé entre desktop et CLI
- **Remote server** sécurisé (gateway vers les providers)

Les trois interfaces partagent le même cœur métier, le même historique, les mêmes projets/workspaces et le même contexte de fichiers.

### Promesse produit

> Un seul outil pour coder avec l'IA : chat agentique, édition de fichiers, exécution de commandes, génération d'artefacts, preview live, et extensibilité (MCP, plugins, skills, agents).

---

## 2. Fonctionnalités attendues

### 2.1 Chat agentique orienté développement

- Historique de sessions par projet
- Messages persistés en SQLite
- Streaming SSE des réponses
- Support du mode "remote", "local" et "auto"
- Fallback automatique remote → local → StubAgent

### 2.2 Lecture / édition de fichiers

- Outil `read_file` avec gating par niveau de confiance
- Outil `write_file` (V1.1)
- Outil `search` / `run_command`
- Contexte projet injecté dans l'agent

### 2.3 Exécution de commandes terminal

- Terminal embarqué (xterm.js)
- Gating par niveau de confiance (Trusted / Untrusted / Sandbox)
- Logs et sorties visibles dans le panneau bas

### 2.4 Génération d'artefacts

- HTML / CSS / JS
- Markdown
- Scripts Python
- Petits outils web

### 2.5 Preview live intégrée

- Panneau droit de l'application desktop
- Rendu HTML/JS/CSS sécurisé (sandbox)
- Rendu Markdown
- Sorties Python textuelles
- Graphiques Python exportés en HTML/PNG (V1.1)

### 2.6 Partage d'état desktop / CLI

- Même daemon local
- Même base SQLite
- Même configuration

### 2.7 Providers

- **Local** : Ollama (obligatoire), architecture extensible pour vLLM / llama.cpp / LM Studio
- **Distant** : OpenAI-compatible générique
- Bascule automatique ou manuelle
- Auto-spawn du runtime local via provider-supervisor

### 2.8 Projets / workspaces

- Création de projets avec niveau de confiance
- Sessions par projet
- Règles de workspace (V1.1)

### 2.9 Extensibilité

- Support MCP servers (global / utilisateur / projet)
- Plugins installables avec manifest
- Commands / slash commands
- Hooks
- Skills / bundles
- Agents spécialisés
- Adaptateurs LSP
- Registre local d'extensions

### 2.10 Remote server sécurisé

- TLS, auth, sessions, permissions
- Rate limiting, audit logs
- Healthchecks, streaming
- Gateway sécurisée vers les providers
- mTLS / VPN privé en V2

---

## 3. Design System

### 3.1 Typographie

- **Police principale** : [Sen](https://fonts.google.com/specimen/Sen)
- Usage : UI, titres, corps de texte, chat, éditeur
- Fallback système : `system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`

### 3.2 Direction artistique

- **Style visuel** : liquide glass inspiré d'Apple (transparence, blur, reflets subtils)
- **Palette dominante** : vert nature, flachi, pas trop saturé
- **Ambiance** : pro, sobre, développeur avancé, calme et concentrée
- **UI** : hiérarchie claire, contraste maîtrisé, espaces généreux, micro-interactions

### 3.3 Couleurs suggérées

| Rôle | Couleur | Usage |
|------|---------|-------|
| Primary | `#4A7C59` | accents, boutons principaux, indicateurs |
| Primary Light | `#6B9E7C` | hover, états actifs |
| Primary Dark | `#2F5236` | fonds sombres, emphasis |
| Background | `#0F1419` | fond principal (dark mode par défaut) |
| Surface | `rgba(255, 255, 255, 0.06)` | panneaux glass |
| Surface Border | `rgba(255, 255, 255, 0.10)` | bordures fines |
| Text Primary | `#F0F2F5` | texte principal |
| Text Secondary | `#9CA3AF` | texte secondaire |
| Danger | `#EF4444` | erreurs, refus |
| Warning | `#F59E0B` | avertissements |
| Info | `#3B82F6` | informations |
| Success | `#10B981` | succès, healthy |

> Les couleurs doivent être paramétrables dans les paramètres de l'application.

### 3.4 Glassmorphism

- `backdrop-filter: blur(20px) saturate(180%)`
- Fond semi-transparent : `rgba(255, 255, 255, 0.05)`
- Bordures fines et lumineuses
- Reflets subtils sur les surfaces actives
- Ombres douces et diffuses

---

## 4. Animations et micro-interactions

### 4.1 Principes généraux

- Animations fluides et subtiles, jamais agressives
- Utilisation de `transform` et `opacity` pour la performance
- Durées courtes : 150–300 ms
- Easing naturel : `cubic-bezier(0.4, 0, 0.2, 1)`

### 4.2 Animations attendues

| Élément | Animation | Détails |
|---------|-----------|---------|
| Ouverture de panneau | Slide + fade | 250 ms, ease-out |
| Apparition messages | Fade + slide up | 200 ms, stagger 50 ms |
| Streaming tokens | Fade in | 80 ms par token |
| ToolCall / ToolResult | Expand + glow | 200 ms, bordure verte subtile |
| Hover boutons | Scale 1.02 + glow | 150 ms |
| Hover items liste | Background fade | 150 ms |
| Indicateur provider actif | Pulse subtil | boucle infinie, 2 s |
| Loading | Shimmer / pulse | sur les surfaces glass |
| Preview artefacts | Fade in | 300 ms |
| Transitions de vue | Cross-fade | 200 ms |
| Sidebar collapse | Width transition | 250 ms, ease-in-out |
| Modals / drawers | Scale + blur backdrop | 200 ms |
| Notifications | Slide from right + fade | 300 ms |
| Terminal | Slide up from bottom | 250 ms |
| Scrollbars | Auto-hide + fade | 200 ms |

### 4.3 Effets spéciaux

- **Liquid glass** : reflets et highlights sur les surfaces actives
- **Glow vert** autour des éléments actifs / focus
- **Shimmer** sur les zones de chargement
- **Micro-parallax** sur les cartes au hover
- **Morphing** des boutons d'action principaux

---

## 5. Architecture UI Desktop

### 5.1 Layout

```
┌─────────────────────────────────────────────────────────────┐
│  Header (mode remote/local, provider actif, settings)     │
├──────────┬───────────────────────────────┬──────────────────┤
│          │                               │                  │
│  Panneau │                               │   Panneau        │
│  gauche  │      Panneau central          │   droit          │
│  (projets│      (chat / éditeur /        │   (artefacts /   │
│   sessions│       tâches)                 │    preview live) │
│          │                               │                  │
│          │                               │                  │
├──────────┴───────────────────────────────┴──────────────────┤
│  Panneau bas (terminal / logs / supervisor)                 │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 Panneaux

- **Gauche** : arborescence projets, sessions, historique
- **Central** : chat, éditeur léger, tâches en cours
- **Droit** : artefacts, preview live, rendu markdown
- **Bas** : terminal embarqué, logs daemon, status supervisor

### 5.3 Composants clés

- **Chat** : bulles de messages, streaming tokens, tool cards, code blocks
- **Éditeur** : Monaco Editor
- **Terminal** : xterm.js
- **Preview** : webview sandboxée
- **Provider indicator** : badge animé indiquant remote/local/auto

---

## 6. Expérience utilisateur

### 6.1 Modes de connexion

- **Remote** : connexion au serveur distant sécurisé
- **Local** : connexion au daemon local
- **Auto** : remote d'abord, fallback local si indisponible

### 6.2 Signalétique du provider actif

- Badge couleur selon le mode
- Texte explicite : "Remote", "Local", "Auto (local)"
- Indicateur d'état : healthy, starting, unhealthy

### 6.3 Permissions et consentements

- Niveau de confiance par projet : Trusted / Untrusted / Sandbox
- Gating des outils selon le niveau
- Avertissements avant exécution de commandes (V1.1)

### 6.4 Gestion des extensions

- UI d'installation / activation / désactivation
- Scope : global / utilisateur / workspace
- Liste des MCP servers
- Liste des plugins, commands, hooks, skills

---

## 7. Architecture technique

### 7.1 Stack

- **Desktop** : Tauri v2 + React + TypeScript
- **CLI** : Rust binaire léger
- **Daemon** : Rust (Axum, SQLite, SSE)
- **Remote server** : Rust (Axum, TLS, auth)
- **Provider supervisor** : Rust
- **Persistence** : SQLite
- **Terminal** : xterm.js
- **Éditeur** : Monaco

### 7.2 Monorepo

```
lochor/
├── apps/
│   ├── desktop/          # Tauri + React + TS
│   └── cli/              # Rust CLI
├── services/
│   ├── daemon/           # Daemon local
│   ├── remote-server/    # Serveur distant
│   └── provider-supervisor/
├── packages/
│   ├── shared-types/
│   ├── sdk/
│   ├── auth/
│   ├── config/
│   ├── storage/
│   ├── events/
│   ├── preview/
│   ├── extensions/
│   ├── mcp/
│   ├── plugin-sdk/
│   ├── command-runtime/
│   ├── hook-runtime/
│   ├── skill-runtime/
│   ├── agent-runtime/
│   ├── rules-runtime/
│   └── lsp-adapters/
├── docs/
│   ├── architecture/
│   └── adr/
└── examples/
```

### 7.3 Communication

- Desktop/CLI ↔ Daemon : HTTP + SSE
- Client ↔ Remote server : HTTPS + SSE
- Daemon ↔ Provider supervisor : in-process

---

## 8. Sécurité

- Daemon local uniquement sur loopback
- Runtime local sur loopback uniquement
- Remote server comme gateway sécurisée
- TLS, auth tokens, rate limiting
- Sandbox de preview
- Gating des outils par niveau de confiance
- Audit logs

---

## 9. Roadmap

### MVP (6–8 semaines)

- Desktop + CLI partageant le même cœur
- Daemon local avec SQLite
- Chat simple et tool-use loop basique
- Preview d'artefacts simple
- Support initial MCP
- Remote server minimal sécurisé

### V1

- UI desktop complète avec 4 pannes
- Système de plugins
- Remote server production-ready
- Fallback robuste
- Preview avancée

### V1.1

- Approval UI interactive
- Règles de workspace
- Plus de providers locaux
- Graphiques Python

### V2

- mTLS / VPN privé
- Marketplace d'extensions
- Agents spécialisés avancés
- Collaboration multi-utilisateur

---

## 10. Notes de design

- Préférer le **dark mode** par défaut
- Interface **épurée et spacieuse**
- **Animations subtiles** pour guider l'attention
- **Feedback immédiat** sur toutes les actions
- **Accessibilité** : contrastes suffisants, focus visibles, raccourcis clavier
- **Responsive** dans les limites d'une app desktop (redimensionnement des panneaux)

---

## 11. Références

- Police : https://fonts.google.com/specimen/Sen
- Inspiration : Apple Liquid Glass, macOS design
- MCP : https://modelcontextprotocol.io/
- Tauri : https://tauri.app/
