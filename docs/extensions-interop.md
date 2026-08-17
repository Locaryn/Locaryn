# Extensions : le format, et ce qu'on accepte des autres

Ce document dit **exactement** ce qu'une extension Locaryn peut contenir, et
quels formats venus d'ailleurs sont acceptés tels quels.

Pour fabriquer une extension, lire d'abord
[`writing-an-extension.md`](writing-an-extension.md). Pour la mécanique interne,
[`architecture/09-extension-model.md`](architecture/09-extension-model.md).

---

## 1. Le principe : ne pas inventer un format de plus

Un écosystème s'est formé autour des agents, et il a produit trois standards
qui se complètent :

| Standard | Ce qu'il transporte | Qui le lit |
| --- | --- | --- |
| **Agent Skills** (`SKILL.md`) | du savoir-faire : des instructions, des références, des scripts | Claude Code, Gemini CLI, OpenCode, Codex, Cursor, Copilot, VS Code, Goose, Letta, Roo Code… |
| **MCP** (`.mcp.json`) | des outils : des serveurs que le modèle appelle | à peu près tout le monde |
| **Plugin Claude Code** (`.claude-plugin/plugin.json`) | un emballage : plusieurs skills, agents, commandes, hooks et serveurs MCP dans un dépôt | Claude Code |

Locaryn lit les trois. Une extension écrite pour Claude Code s'installe ici
sans modification, et une extension écrite ici reste lisible ailleurs tant
qu'elle s'en tient à la partie commune.

Ce que Locaryn ajoute — et c'est le seul ajout — est une **couche
d'interface** : une extension peut décrire ce qu'elle veut voir apparaître à
l'écran, et l'utilisateur peut la modeler. Aucun des trois standards ne couvre
cela, parce qu'ils viennent d'outils en ligne de commande. Cet ajout est
facultatif : une extension qui l'ignore fonctionne quand même.

---

## 2. Agent Skills — le savoir-faire

Le format est celui de [agentskills.io](https://agentskills.io) : un dossier
qui contient un `SKILL.md`.

```
ma-competence/
├── SKILL.md          # obligatoire : métadonnées + instructions
├── scripts/          # facultatif : du code exécutable
├── references/       # facultatif : de la documentation
└── assets/           # facultatif : modèles, ressources
```

Le `SKILL.md` commence par un en-tête YAML. **Les six champs du standard**,
et eux seuls, garantissent la portabilité :

| Champ | Obligatoire | Rôle |
| --- | --- | --- |
| `name` | oui | identifiant, en kebab-case |
| `description` | oui | ce que la compétence sait faire, et quand s'en servir. C'est le seul texte lu au démarrage : il décide si la compétence sera chargée |
| `license` | non | identifiant SPDX |
| `compatibility` | non | prérequis d'environnement, 500 caractères au plus |
| `metadata` | non | ce que l'auteur veut y mettre |
| `allowed-tools` | non | les outils que la compétence peut appeler |

```markdown
---
name: revue-de-code
description: Relit un diff et signale les défauts de correction, pas de style. À utiliser avant d'ouvrir une demande de fusion.
license: Apache-2.0
---

# Revue de code

1. Lire le diff en entier avant de commenter.
2. …
```

Le chargement se fait en trois temps — c'est ce qui permet d'en garder
beaucoup sans les payer : au démarrage seuls le nom et la description sont
lus ; le corps n'entre en contexte que lorsqu'une tâche y correspond ; les
scripts et les références ne se chargent qu'à l'exécution.

Locaryn cherche les compétences dans, par ordre :

```
<extension>/skills/<nom>/SKILL.md
<extension>/SKILL.md          # l'extension est elle-même une compétence
```

**Les champs hors standard sont acceptés et ignorés**, jamais une erreur : une
compétence écrite pour un autre outil ne doit pas refuser de s'installer ici à
cause d'un champ qu'on ne sait pas lire.

---

## 3. MCP — les outils

Un serveur MCP se déclare dans `.mcp.json` (ou `mcp.json`) à la racine de
l'extension, au format commun :

```json
{
  "mcpServers": {
    "mon-serveur": {
      "command": "node",
      "args": ["./serveur.js"],
      "env": { "CLE": "valeur" }
    }
  }
}
```

Les outils qu'il expose deviennent appelables par le modèle, sous le préfixe
du serveur. Les permissions du § 6 s'appliquent : un serveur MCP qui touche au
réseau ou au disque doit le demander.

---

## 4. Plugin Claude Code — l'emballage

Un dépôt contenant `.claude-plugin/plugin.json` est reconnu comme extension.
Seul `name` est obligatoire ; le reste indique où trouver les composants.

| Champ | Défaut | Ce que Locaryn en fait |
| --- | --- | --- |
| `name` | — | identifiant de l'extension |
| `displayName` | `name` | nom affiché |
| `version`, `description`, `author`, `homepage`, `repository`, `license`, `keywords` | — | métadonnées, reprises telles quelles |
| `defaultEnabled` | `true` | l'extension démarre active ou non |
| `skills` | `skills/` | compétences (§ 2). **S'ajoute** au dossier par défaut |
| `commands` | `commands/` | fichiers `.md` plats, exposés comme commandes |
| `agents` | `agents/` | sous-agents, en Markdown avec en-tête |
| `hooks` | `hooks/hooks.json` | crochets d'événements |
| `mcpServers` | `.mcp.json` | serveurs MCP (§ 3) |
| `lspServers` | `.lsp.json` | serveurs de langage |
| `outputStyles`, `workflows`, `experimental.*` | — | acceptés, non utilisés à ce jour |

Tous les chemins sont relatifs et commencent par `./`. `commands`, `agents`,
`workflows` et `outputStyles` **remplacent** le dossier par défaut ;
`skills` **s'y ajoute** ; `hooks`, `mcpServers` et `lspServers` ont leurs
propres règles de fusion.

Locaryn accepte aussi son propre manifeste, `plugin.json` à la racine, qui a le
même rôle et ajoute la couche d'interface décrite ci-dessous. Quand les deux
existent, le manifeste Locaryn l'emporte pour ce qui lui est propre, et
`.claude-plugin/plugin.json` reste la source pour le reste.

---

## 5. La couche d'interface — ce que Locaryn ajoute

Les trois standards viennent d'outils en ligne de commande : aucun ne sait dire
« mets un bouton ici ». Locaryn n'est pas un terminal, et une extension qui
apporte la génération d'images n'a rien à faire si personne ne peut la voir.

### 5.1 Capacités

```json
{
  "capabilities": ["image-gen"]
}
```

Une capacité est un mot que l'interface comprend. Elle décide de la présence
d'un écran : le Studio n'existe que si une extension installée sait générer
quelque chose ; la retirer retire l'écran, sur l'ordinateur **et** sur le
téléphone.

Capacités reconnues à ce jour : `image-gen`, `image-editor`, `voice-tts`,
`voice-cloning`, `music-gen`, `video-gen`, `3d-gen`, `vision-ocr`,
`text-analysis`, `translation`, `rag-qa`, `model-training`,
`ssh-remote-exec`, `travel-tunnel`.

Une capacité déclarée sans moteur derrière n'apporte aucun outil au modèle :
mieux vaut que le modèle dise honnêtement qu'il ne sait pas faire que
d'appeler un outil qui échouera.

### 5.2 Contributions d'interface

```json
{
  "ui_contributions": {
    "nav_items":     [{ "id": "studio", "label": "Studio", "icon": "studio" }],
    "studio_tabs":   [{ "id": "image",  "label": "Image",  "icon": "image" }],
    "composer_actions": [
      {
        "id": "dictate",
        "label": "Dicter",
        "icon": "mic",
        "action": "tool",
        "value": "transcribe_audio",
        "hint": "Dicter au lieu d'écrire"
      }
    ],
    "settings_sections": [
      {
        "id": "dictee",
        "label": "Dictée",
        "fields": [
          { "id": "model", "type": "model", "label": "Modèle d'écoute" },
          { "id": "auto_send", "type": "boolean", "label": "Envoyer après la dictée" }
        ]
      }
    ]
  }
}
```

| Contribution | Où elle apparaît | Ordinateur | Téléphone |
| --- | --- | --- | --- |
| `nav_items` | menu principal | oui | oui |
| `studio_tabs` | onglets du Studio | oui | oui |
| `composer_actions` | à côté du champ de saisie | oui | oui |
| `settings_sections` | réglages, en sous-écran | oui | oui |

### 5.2 bis  Ce que fait un bouton de composeur

Deux comportements, pas plus :

| `action` | Effet | `value` |
| --- | --- | --- |
| `insert` | écrit `value` dans le champ de saisie | le texte à insérer |
| `tool` | appelle l'outil nommé avec ce que le champ contient, et met la réponse à la place | le nom de l'outil |

L'outil est cherché parmi les serveurs MCP démarrés de toutes les extensions
actives : le manifeste nomme un outil, pas un serveur — celui qui écrit
l'extension sait ce qu'elle expose, pas sous quel nom son serveur tournera
chez les autres.

Il n'y a pas de troisième comportement, et il n'y en aura pas : faire tourner
du code d'extension dans l'interface reviendrait à lui donner l'écran entier.

`icon` est un nom du jeu partagé (`@locaryn/ui-core`), jamais une image
fournie par l'extension : le jeu est dessiné d'une seule main, et une icône
importée jurerait. Les noms disponibles sont listés dans
`packages-ui/core/src/icons.tsx`.

**Une contribution ne s'impose pas.** L'utilisateur peut masquer n'importe
quelle entrée depuis les réglages de l'extension : une extension décrit ce
qu'elle propose, elle ne décide pas de l'écran de quelqu'un d'autre.

### 5.3 Types de champs de réglage

| `type` | Rendu | Valeur |
| --- | --- | --- |
| `boolean` | interrupteur | `"true"` / `"false"` |
| `select` | liste, à partir de `options: ["a", "b"]` | la valeur choisie |
| `model` | liste des modèles installés sur le serveur | le nom du modèle |
| `string`, `number`, `prompt` | champ texte | chaîne |

Quatre rendus pour six mots : `number` et `prompt` s'affichent comme du texte.
Mieux vaut un champ honnête qu'un rendu promis et absent — le jour où l'écran
saura montrer un curseur numérique, `number` en profitera sans qu'aucun
manifeste change.

Les valeurs sont rangées **par le serveur**, dans le dossier de l'extension
(`.data/config.json`), jamais par le client : un réglage choisi sur
l'ordinateur vaut sur le téléphone, et retirer l'extension emporte ses
réglages avec elle.

`key` s'écrit aussi `id`, `title` s'écrit aussi `label`, `kind` s'écrit aussi
`type` : les deux orthographes sont lues.

---

## 6. Permissions

Une extension demande ce dont elle a besoin, et rien n'est accordé
implicitement :

```json
{
  "permissions": {
    "files_read":  { "reason": "Lire les images à retoucher" },
    "files_write": false,
    "network":     { "reason": "Télécharger les poids du modèle" },
    "shell":       false
  }
}
```

`false` vaut refus explicite. Une permission demandée est présentée à
l'utilisateur au moment de l'installation, avec sa raison ; sans raison, la
demande est affichée comme non motivée — c'est délibéré.

---

## 7. État de la mise en œuvre

Ce tableau est la partie la plus importante de ce document : il dit ce qui
marche, pas ce qu'on voudrait.

| Élément | État |
| --- | --- |
| Manifeste Locaryn `plugin.json` (nom, version, capacités) | **fait** |
| `ui_contributions.nav_items` et `studio_tabs` | **fait** (lus, et le Studio suit les capacités) |
| Installation depuis un dépôt du catalogue (`propriétaire/dépôt`) | **fait** |
| Installation depuis un dossier local | **fait** |
| Activation, désactivation, retrait, persistance | **fait** |
| Serveurs MCP (`mcp.json` partagé application/CLI/service) | **fait** |
| Capacité → outil pour le modèle | **partiel** : `image-gen` et `voice-tts` seulement |
| Lecture de `SKILL.md` (Agent Skills) | **à faire** |
| Lecture de `.claude-plugin/plugin.json` | **à faire** |
| `agents/`, `commands/`, `hooks/` d'un plugin Claude Code | **à faire** |
| `composer_actions` (`insert` et `tool`) | **fait**, ordinateur et téléphone |
| `settings_sections` (`boolean`, `select`, `model`, texte) | **fait**, ordinateur et téléphone |
| Masquage d'une contribution par l'utilisateur | **à faire** |

---

## 8. Publier

Une extension est un dépôt Git. Rien à soumettre, aucune file d'attente :

1. Publiez le dépôt.
2. L'utilisateur colle `propriétaire/dépôt` dans Réglages → Extensions, ou
   choisit l'extension dans le catalogue si elle y figure.

Le service télécharge l'archive, vérifie qu'elle contient un manifeste, et
refuse tout ce qui ne ressemble pas à un dépôt : c'est du code qu'il exécute.
