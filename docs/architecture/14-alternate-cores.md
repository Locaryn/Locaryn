# 14 — Noyaux alternatifs (OpenClaw, Hermes Agent…)

> Proposition d'intégration : installer un **noyau tiers** depuis Réglages →
> Extensions, l'utiliser par session, **sans jamais remplacer le noyau
> Locaryn**. Locaryn reste l'hôte : mêmes API, même stockage, même UI, mêmes
> permissions. Les conversations normales continuent à côté, inchangées.

---

## 1. Ce que l'on veut

Un utilisateur doit pouvoir, depuis **Réglages → Extensions**, installer un
**noyau alternatif** — OpenClaw, Hermes Agent, puis d'autres — qui :

- change le **comportement** de l'agent (sa boucle, sa mémoire, ses skills,
  son système de « pensée ») pour les conversations qui l'utilisent ;
- fait tourner ses propres **skills** et intégrations (ex. Home Assistant) ;
- **n'écrase pas** le noyau Locaryn : une session choisit son noyau, et les
  conversations ordinaires continuent avec le noyau natif, à côté ;
- s'appuie sur les **mêmes API Locaryn** (daemon/desktop, SSE, stockage,
  preview, terminal) — c'est Locaryn qui reste le système nerveux.

Cas d'usage concret : « je veux piloter mon Home Assistant depuis Locaryn via
OpenClaw, avec sa mémoire persistante et ses skills, tout en continuant à
discuter normalement dans d'autres sessions ».

---

## 2. Constat sur l'existant

Ce que Locaryn a déjà, et qui sert de socle :

| Brique | État | Rôle pour les noyaux |
| --- | --- | --- |
| `morph.json` + registry d'extensions | Fonctionnel | Le manifeste peut accueillir une section `core` ; install/enable/remove déjà en place |
| Écran Réglages → Extensions | Fonctionnel (desktop + mobile) | Point d'entrée demandé : installer le noyau, le configurer, le démarrer |
| Trait `Agent` (`agent-runtime`) | Fonctionnel | `OllamaAgent`, `OpenAiCompatAgent`, `StubAgent`. **Un noyau externe peut s'y brancher comme n'importe quel agent** |
| `OpenAiCompatAgent` | Fonctionnel | Locaryn parle **déjà** le format OpenAI (chat/completions, SSE) |
| Gating d'approbation | Fonctionnel | `run_command`, `write_file`, outils MCP passent par l'approbation |
| Sessions (SQLite) | Fonctionnel | Il manque juste une colonne `core_id` |
| UI de chat, tool cards, streaming | Fonctionnel | Rien à changer : le pont émet les mêmes `StreamEvent` |

Deux lacunes à combler : (1) la sélection d'agent est codée en dur dans
`send_message` (daemon et desktop) — pas de notion de noyau par session ;
(2) le manifeste n'a pas de concept de « noyau ».

---

## 3. Terrain : ce que les cibles exposent réellement

La recherche terrain change tout : **les deux cibles exposent une API HTTP
locale, OpenAI-compatible, avec streaming SSE et continuité de session**.
Locaryn n'a donc pas besoin d'embarquer leur code : il les *pilote*.

### OpenClaw

- **OpenResponses API** : `POST /v1/responses` sur le port du Gateway
  (loopback), compatible OpenAI Responses — `input`, `instructions`, `tools`
  (fonctions client), `stream: true` (SSE), `previous_response_id` /
  `user` pour la continuité de session, `input_image`/`input_file`.
- Même surface : `GET /v1/models`, `POST /v1/chat/completions`,
  `POST /v1/embeddings`.
- Auth : `Authorization: Bearer <secret>` (mode `shared-secret`).
- Un appel = un **run Gateway normal** : mémoire persistante, skills,
  sub-agents, cron, canaux (dont Home Assistant) fonctionnent à l'identique.
- Skills : `~/.openclaw/skills/<name>/SKILL.md` (même convention markdown que
  Locaryn), `openclaw skills install @owner/<slug>`, registre ClawHub
  (clawhub.ai), `skills.load.extraDirs`.

### Hermes Agent

- **API server** (activé via `API_SERVER_ENABLED=true` + `API_SERVER_KEY`) :
  `http://127.0.0.1:8642/v1/chat/completions` (OpenAI-compatible, SSE,
  événement custom `hermes.tool.progress`), `/v1/responses`, et surtout une
  **Runs API** taillée pour un hôte : `POST /v1/runs`, `GET /v1/runs/{id}/events`
  (SSE : tokens, outils, sous-agents), `POST /v1/runs/{id}/stop`,
  `POST /v1/runs/{id}/approval`, `GET /v1/capabilities`, `GET /health`.
- Continuité : `conversation`, `previous_response_id`, `session_id`.
- Home Assistant : **plugin de plateforme `homeassistant` fourni**.
- Skills : skills fournis + « boucle d'apprentissage » qui crée/améliore les
  skills ; `hermes skills` ; répertoire `~/.hermes/skills`.
- Mémoire : gestionnaire mémoire à trois niveaux, providers de mémoire
  enfichables — tout cela vit **côté Hermes**, Locaryn n'a rien à répliquer.

### Conséquence

Le pont n'a pas besoin de connaître OpenClaw ni Hermes en profondeur : il a
besoin d'un **driver** par dialecte (OpenResponses, Runs, Chat Completions) et
d'un **cycle de vie** (démarrer le processus, attendre le healthcheck,
configurer le secret, mapper les sessions). C'est exactement le métier que
Locaryn fait déjà pour Ollama/llama-server via le provider-supervisor.

---

## 4. Architecture proposée

### 4.1 Le concept : un « noyau » est une extension avec un driver

Une extension de noyau = un `morph.json` enrichi d'une section `core` :

```json
{
  "schema": "https://locaryn.dev/schema/morph.json/v0.1",
  "apiVersion": "0.1",
  "name": "locaryn-core-openclaw",
  "version": "1.0.0",
  "description": "Noyau OpenClaw : mémoire persistante, skills, Home Assistant",
  "permissions": {
    "network": { "reason": "Pilote le gateway OpenClaw en local (loopback)" },
    "shell": { "reason": "Lance et supervise le processus openclaw", "scope": "always" },
    "env": ["OPENCLAW_GATEWAY_PASSWORD"]
  },
  "capabilities": ["core", "assistant", "home-assistant", "memory"],
  "ui_contributions": {
    "settings_sections": [
      {
        "id": "noyau-openclaw",
        "label": "Noyau OpenClaw",
        "fields": [
          { "id": "auto_start", "type": "boolean", "label": "Démarrer avec Locaryn" },
          { "id": "agent", "type": "string", "label": "Agent OpenClaw (main)" }
        ]
      }
    ]
  },
  "core": {
    "driver": "responses",
    "api_url": "http://127.0.0.1:18789/v1/responses",
    "port": 18789,
    "install": {
      "kind": "npm",
      "package": "openclaw",
      "version": "1.2.3",
      "fallback": "existing"
    },
    "lifecycle": {
      "start": ["openclaw", "gateway", "--port", "{{port}}"],
      "env": { "OPENCLAW_GATEWAY_PASSWORD": "{{token}}" },
      "health": { "method": "GET", "url": "{{models_url}}", "retries": 30, "interval_ms": 1000 }
    },
    "session": { "routing": "user", "max_sessions": 20 },
    "skills": {
      "registry": "clawhub",
      "install_dir": "~/.openclaw/skills",
      "native": true
    },
    "tools": { "client_tools": false, "approval": "locaryn" }
  }
}
```

Champs clés :

- `driver` : `responses` (OpenClaw), `runs` (Hermes), `chat_completions`
  (générique OpenAI-compatible) — l'implémentation de pont vit dans Locaryn,
  l'extension est **déclarative**.
- `install` : d'où vient le binaire/paquet (`binary`, `pip`, `npm`, `existing`
  = l'utilisateur l'a déjà). Locaryn télécharge, vérifie la somme, installe.
- `lifecycle` : comment le démarrer, healthcheck, arrêt.
- `session.routing` : comment mapper une session Locaryn → session du noyau
  (`user` = champ `user` stable, `conversation` = nom, `response` =
  `previous_response_id`).
- `skills.registry` : d'où viennent les skills (`clawhub`, `hermes`,
  `folder`), où ils s'installent, et s'ils restent natifs au noyau.

### 4.2 Le pont : un `Agent` Locaryn comme les autres

Nouveau crate `packages/core-bridge` (dans le dépôt principal) :

```
core-bridge/
├── manifest.rs     # section `core` du morph.json (validation)
├── manager.rs      # CoreManager : install/start/stop/health, mappage sessions
├── driver.rs       # trait CoreDriver : health(), send(), stop(), approve()
├── responses.rs    # driver OpenResponses (OpenClaw) — tools client, SSE
├── runs.rs         # driver Runs (Hermes) — SSE d'événements, approval, stop
├── chat.rs         # driver Chat Completions (générique)
└── agent.rs        # ExternalCoreAgent : implémente le trait Agent existant
```

Le point crucial : `ExternalCoreAgent` implémente le **même trait `Agent`**
que `OllamaAgent`. Tout l'aval fonctionne sans changement :

- le daemon et le desktop choisissent l'agent selon `session.core_id` au lieu
  du code en dur actuel ;
- les événements sortent en `StreamEvent` (tokens, ToolCall, ToolResult)
  → tool cards, persistance, annulation, métriques inchangés.

### 4.3 Outils : partage clair entre Locaryn et le noyau

**Décision de revue (v1) : le pont ne déclare aucun outil Locaryn au noyau**
(`client_tools: false` par défaut). OpenClaw et Hermes ont déjà leurs propres
outils terminal/fichiers/web : déclarer en plus `run_command` ou `write_file`
donnerait au modèle deux chemins pour la même action, avec deux politiques
— incohérent et trompeur.

| Outil | Qui l'exécute | Gating |
| --- | --- | --- |
| Outils du noyau (terminal, fichiers, mémoire, skills, Home Assistant, navigateur…) | **Le noyau** | Garde-fous du noyau (approvals Hermes, scopes opérateur OpenClaw) ; Locaryn relaye les décisions en attente vers l'UI |
| Outils *client* (opt-in, ex. exposer un MCP serveur Locaryn au noyau) | Locaryn les exécute quand le noyau les appelle (OpenClaw renvoie un `function_call`) | Approbation Locaryn |

Les appels d'outils du noyau arrivent dans l'UI comme des tool cards, comme
aujourd'hui (progression SSE traduite en `StreamEvent`).

**Conséquence assumée** : le `TrustLevel` Locaryn (Trusted/Untrusted/Sandbox)
ne s'applique plus aux outils exécutés dans le noyau — le noyau a ses propres
règles. À l'activation, l'UI l'écrit noir sur blanc (« ce noyau exécute ses
propres outils avec ses propres règles d'approbation »). Le TrustLevel est
transmis au noyau en texte (`instructions`) ; un projet `Sandbox` refuse de
joindre un noyau.

### 4.4 Sessions : une colonne, zéro rupture

- Migration `0012_core_id.sql` : `ALTER TABLE sessions ADD COLUMN core_id TEXT`
  (NULL = noyau Locaryn natif).
- Création de session : l'UI propose « Noyau : Locaryn | OpenClaw | Hermes ».
- Le badge du noyau s'affiche dans la liste des sessions et dans l'en-tête du
  chat.
- Le CLI : `locaryn sessions new --core openclaw` ; les sessions à noyau sont
  lisibles depuis le desktop, le mobile et le CLI (parité existante).

### 4.5 Skills : le catalogue voyage avec l'extension

L'extension de noyau embarque (ou sait interroger) un **index de skills** :

- OpenClaw : registre ClawHub — onglet « Skills » dans la carte du noyau :
  chercher, installer (`openclaw skills install @owner/<slug>` ou écriture
  directe dans `~/.openclaw/skills/`), désactiver.
- Hermes : skills fournis + catalogue `hermes skills` (hub) — même onglet.
- Format commun : `SKILL.md` en frontmatter YAML — la **même convention que
  Locaryn** (document 09). Un skill natif au noyau reste natif (il s'exécute
  dans le contexte du noyau, avec sa mémoire) ; un skill au format Locaryn
  pur peut aussi être installé comme extension Locaryn classique.

Le principe : **les skills des écosystèmes s'installent depuis Locaryn, mais
tournent dans leur écosystème** — pas de conversion perdue, pas de double
maintien.

### 4.6 Mémoire et « pensée »

- La mémoire (faits, préférences, pensées) vit **dans le noyau** (`~/.openclaw`,
  `~/.hermes`) : Locaryn n'en fait pas une copie, il la *relie* (chaque session
  Locaryn est routée vers une session stable du noyau, donc la mémoire suit).
- La « pensée » d'OpenClaw et les événements de progression d'Hermes
  (`hermes.tool.progress`, items `reasoning`) sont traduits en `StreamEvent`
  existants (token de raisonnement replié, tool cards) — le comportement
  visuel de l'app ne change pas.

### 4.7 Sécurité

- Les noyaux ne sont joints **que sur loopback** (OpenClaw et Hermes le font
  par défaut) ; Locaryn refuse une URL non-loopback à l'installation.
- **Jeton généré par Locaryn** à l'install (CSPRNG), injecté dans la config du
  noyau ; les permissions du manifeste (`network`, `shell`, `env`) passent par
  la fenêtre de permissions existante.
- **Avertissement renforcé à l'activation** : une extension `core` fait
  tourner un programme tiers avec les droits de l'utilisateur et un accès
  réseau. La fenêtre de permissions affiche un niveau « élevé » explicite.
- **Données sortantes** : les messages d'une session à noyau transitent par le
  fournisseur configuré *dans le noyau* (ex. API Anthropic pour OpenClaw).
  L'UI le dit dans la carte du noyau et à la création d'une session à noyau —
  les sessions natives Locaryn, elles, restent sur les providers Locaryn.
- **Chaîne d'approvisionnement** : version du paquet pinnée
  (`install.version`), somme de contrôle vérifiée quand disponible (binaire),
  et jamais de shell dans `lifecycle.start` (liste d'arguments + env contrôlé,
  kill du groupe de processus à l'arrêt).
- Les appels d'outils en attente relaient par l'approbation du noyau, relayée
  dans l'UI Locaryn (Hermes) ; un noyau n'a pas accès aux données d'autres
  extensions ni aux sessions natives ; la désinstallation arrête le processus
  et retire les fichiers.

---

## 5. Parcours utilisateur

1. **Réglages → Extensions → Découvrir** : deux entrées distinctes —
   « Noyau OpenClaw » et « Noyau Hermes » — servies par la source de catalogue
   officielle `Locaryn/locaryn-cores` (chacune pointe son sous-chemin
   `#cores/openclaw`, `#cores/hermes` ; la racine du dépôt n'est pas un
   plugin). Bouton « + Depuis un dépôt GitHub » accepte aussi
   `github:Locaryn/locaryn-cores#cores/openclaw`.
2. Fenêtre de permissions → **Activer**. Locaryn télécharge le binaire,
   génère le jeton, lance le noyau, attend le healthcheck (statut
   « Noyau OpenClaw : en cours d'exécution »).
3. Onglet **Skills** de la carte du noyau → installer « home-assistant »,
   « calendar », etc.
4. **Nouvelle session** → choisir « Noyau : OpenClaw » → discuter : la mémoire
   d'OpenClaw, ses skills, son contrôle Home Assistant fonctionnent, dans
   l'UI Locaryn.
5. Les autres sessions restent sur le noyau Locaryn, inchangées.

---

## 6. Découpage

### Phase A — Hôte Locaryn (dépôt principal)

1. `morph.json` : section `core` + validation (`manifest.rs`).
2. `packages/core-bridge` : drivers `responses` / `runs` / `chat_completions`,
   `CoreManager`, `ExternalCoreAgent`, sérialisation par session (file),
   boucle client-tools bornée (opt-in) — voir §9.
3. Migration `0012_core_id.sql` ; sélection d'agent par `session.core_id`
   (daemon + desktop), **sans fallback silencieux** : noyau choisi mais
   indisponible = message clair + bouton « démarrer le noyau ».
4. UI : carte « Noyau » dans Réglages → Extensions (statut, démarrer/arrêter,
   réglages, avertissement de niveau élevé, mention du fournisseur du noyau),
   choix du noyau à la création de session, badge dans le chat, onglet
   Skills. Mobile : même écran (les `settings_sections` existent déjà).
5. CLI : `locaryn sessions new --core`, `locaryn cores list/start/stop`.
6. `CoreManager` instancié **dans le daemon aussi** (le daemon n'a pas de
   runtime d'extensions aujourd'hui) : il lit les manifestes `core` sur
   disque, supervise les processus, et sert les sessions CLI. Le desktop
   garde l'installation ; le daemon devient le superviseur des processus.
7. Source de catalogue « Officiel Locaryn » : `locaryn-cores/catalog.json`
   liste les entrées des deux noyaux (le catalogue Découvrir existant sait
   déjà lire des index).
8. **Fake core** (`locaryn-cores/tests/fake-core/`) : mini-serveur HTTP
   parlant les dialectes `responses`/`runs`, pour la CI du pont sans réseau ;
   tests d'intégration `#[ignore]` contre les vrais noyaux.

### Phase B — Dépôt d'extension `Locaryn/locaryn-cores` (nouveau repo)

Contenu actuel (déjà initialisé, commit `16724b6`) :

```
locaryn-cores/
├── README.md                 # présentation + installation
├── cores/
│   ├── openclaw/             # morph.json (driver responses) + README
│   └── hermes/               # morph.json (driver runs) + README
├── skills/
│   ├── openclaw-index.json   # index de départ (ClawHub interrogé à la volée)
│   └── hermes-index.json
├── docs/
│   ├── integration.md        # ce document, version publique
│   └── drivers.md            # contrat du pont hôte ↔ noyau
└── LICENSE                  # Apache-2.0 (aligné sur le cœur)
```

À ajouter : `catalog.json` (source de catalogue officielle, deux entrées),
`tests/fake-core/` (voir Phase A.8). Le dépôt est le **point de publication** :
les sous-chemins `#cores/openclaw` et `#cores/hermes` sont les sources
installables, et la vitrine des skills.

### Phase C — Recettes et vitrine

- Recette Home Assistant : config OpenClaw (canal HA, skills), capture
  d'écran, guide pas à pas.
- Index des skills utiles (les 5 400+ de ClawHub sont filtrables ; on publie
  une sélection vérifiée).

---

## 7. Ce qui ne change pas

- Le noyau Locaryn reste le défaut et le seul pour les sessions sans `core_id`.
- Les API (daemon `/v1/*`, SSE), le stockage SQLite, la preview, le terminal,
  MCP, les extensions classiques : inchangés.
- La parité desktop/CLI/mobile est conservée (le pont est dans le cœur Rust,
  pas dans l'UI).

---

## 8. Risques

| Risque | Mitigation |
| --- | --- |
| API des noyaux qui évolue | Driver par dialecte, pas par produit ; `GET /v1/capabilities` (Hermes) pour négocier ; tests d'intégration dans la CI de `locaryn-cores` (fake core) |
| OpenClaw n'expose pas de binaire officiel par plateforme | Mode `existing` (l'utilisateur installe via `npm i -g openclaw`), et on documente |
| Le noyau consomme beaucoup de ressources | Supervision à l'image du provider-supervisor : démarrage à la demande, arrêt après inactivité, statut visible |
| Skills non vérifiés (injection) | Skills installés par l'utilisateur explicitement ; contenu des skills traité comme données non fiables (déjà la règle Locaryn pour les bundles) |
| **Annulation côté OpenClaw** : pas d'endpoint `stop` documenté ; couper le SSE laisse le run du gateway continuer (effets de bord) | Abandon + journalisation ; vérifier les session tools du gateway (v1.1) ; l'UI affiche « le noyau continue peut-être en arrière-plan » |
| **Session noyau perdue** (réinstallation, wipe `~/.openclaw`) → conversation sans contexte | Locaryn persiste l'historique de toute façon : ré-hydratation à la reconnexion des messages non accusés (`last_sent_message_id`, voir contrat §4) |
| **Deux messages concurrents** sur la même session noyau → désordre | Sérialisation par session (file, comme `cancel_map` existant) |
| **Mode remote** : où tourne le noyau ? | v1 : noyaux disponibles en mode local uniquement (message clair en remote) ; serveur-multi-utilisateur + noyaux = v2 |
| **Fournisseur du noyau vs providers Locaryn** : deux configs de modèle | v1 : la config modèle reste native au noyau (pas de sync) ; override par requête (Hermes) en v1.1 |
| Chaîne d'approvisionnement npm/pip | Version pinnée, somme vérifiée (binaire), avertissement « niveau élevé » à l'activation |
| Métriques fausses | Mapper `usage` des réponses noyau vers `MessageEnd` (tokens_in/out) |

---

## 9. Revue avant implémentation — décisions actées

Relu le 2026-08-17, avant tout code. Les points ci-dessous sont tranchés et
font partie de la spécification ; ce qui reste ouvert est listé à la fin.

| # | Décision | Conséquence |
| --- | --- | --- |
| D1 | **`client_tools: false` par défaut** : le pont ne déclare pas d'outils Locaryn au noyau | Pas de double chemin `run_command` ; le gating Locaryn ne couvre pas les outils du noyau — assumé et affiché à l'activation |
| D2 | **Pas de fallback silencieux** vers le noyau natif quand un noyau choisi est indisponible | Message d'erreur clair + action « démarrer le noyau » |
| D3 | **Sérialisation par session** (une file par session à noyau) | Pas de runs concurrents désordonnés sur la même session noyau |
| D4 | **Le daemon supervise les processus** (CoreManager instancié daemon + desktop, mêmes répertoires de plugins) | Les sessions CLI à noyau fonctionnent ; un seul superviseur de processus |
| D5 | **Noyaux en mode local uniquement (v1)** | Mode remote = message clair ; multi-utilisateur + noyaux en v2 |
| D6 | **Config modèle native au noyau (v1)** | Pas de sync des providers Locaryn ↔ config noyau ; override par requête en v1.1 (Hermes `model`/`provider`) |
| D7 | **Annulation** : `runs` → `POST /v1/runs/{id}/stop` ; `responses` → abandon + log | Risque d'effets de bord OpenClaw documenté dans l'UI |
| D8 | **Ré-hydratation** : rejouer les messages non accusés au (re)branchement du noyau | `last_sent_message_id` par session, dans la table de mappage |
| D9 | **Sessions éphémères** : clé noyau jetable (uuid aléatoire), nettoyage si le dialecte le permet | La promesse « rien ne persiste » tient aussi avec un noyau |
| D10 | **Métriques** : mapper `usage` → `MessageEnd` | Compteurs vrais dans l'UI et la DB |
| D11 | **Avertissement « niveau élevé » + mention du fournisseur** à l'activation et à la création d'une session à noyau | Consentement éclairé (le noyau envoie les messages à *son* fournisseur) |
| D12 | **Version pinnée** (`install.version`), checksum (binaire), pas de shell dans `lifecycle.start` | Chaîne d'approvisionnement maîtrisée |
| D13 | **Installation par sous-chemin** (`github:Locaryn/locaryn-cores#cores/openclaw`) + `catalog.json` officiel | Deux entrées distinctes dans Découvrir ; la racine du dépôt n'est pas un plugin |
| D14 | **Fake core en CI** (`tests/fake-core/`) | Le pont est testable sans réseau ni vrais noyaux |

### Reste ouvert (à trancher en Phase A, avec preuves)

- **Boucle client-tools opt-in** : quand un noyau déclare `client_tools: true`
  (ex. exposer un MCP serveur Locaryn), combien de tours max, quel timeout,
  quelle UX de « l'outil tourne chez Locaryn » ? À caler sur la boucle
  existante de `tool_loop.rs` (10 rounds aujourd'hui).
- **Switcher de noyau en cours de session** : autorisé ? (proposition : oui,
  avec avertissement — la session noyau repart vide, l'historique Locaryn
  reste visible ; un « rejouer avec un autre noyau » est un fork).
- **Terminologie UI** : « Noyau » vs « Moteur » déjà utilisé pour les
  providers locaux — vérifier la collision dans la copie des réglages.
