# Écrire une extension Locaryn

Ce guide s'adresse à qui veut **fabriquer** une extension. Pour le format exact
et ce que Locaryn accepte des autres écosystèmes — Agent Skills, MCP, plugins
Claude Code — voir [`extensions-interop.md`](extensions-interop.md). Pour la
mécanique interne — cycle de vie, bac à sable, chargement runtime — voir
[`architecture/09-extension-model.md`](architecture/09-extension-model.md).

---

## Le principe : deux produits, deux dépôts

Locaryn est une application d'intelligence artificielle locale. Une extension
est un produit **distinct**, avec son propre dépôt, son propre rythme de
publication et son propre auteur.

L'application ne nomme aucune extension en particulier. Pas d'onglet dédié,
pas de commande spécifique, pas de type nommé d'après tel ou tel greffon.
Cette règle n'est pas cosmétique : dès qu'une application cite une extension
dans son code, elle en devient responsable — il faut la maintenir, la tester,
la livrer, et expliquer à ceux qui ne l'utilisent pas pourquoi elle est là.

Concrètement, cela veut dire qu'une extension **ne se soumet nulle part**.
Vous publiez un dépôt Git ; l'utilisateur colle son adresse. Il n'y a ni
validation, ni file d'attente, ni magasin central obligatoire.

---

## Comment quelqu'un installe la vôtre

1. **Réglages → Extensions → Ajouter**
2. Il colle l'adresse de votre dépôt.
3. L'application lit le manifeste **avant** d'installer et affiche ce que
   l'extension déclare : nom, version, auteur, écosystème, serveurs MCP et
   permissions demandées.
4. Il accepte — ou non. L'extension arrive **désactivée** : les permissions
   sont un second geste, délibéré.

Formes d'adresse acceptées :

| Forme | Exemple |
|---|---|
| `owner/repo` | `jane/my-plugin` |
| URL GitHub | `https://github.com/jane/my-plugin` |
| Sous-dossier d'un dépôt | `https://github.com/jane/monorepo/tree/main/plugins/x` |
| Version épinglée | `github:jane/my-plugin@v1.2.0` |
| Dossier local | `./mon-plugin` (pratique pendant le développement) |

---

## L'extension minimale

Un dépôt, un fichier :

```json
{
  "apiVersion": "0.1",
  "name": "mon-extension",
  "version": "1.0.0",
  "description": "Ce qu'elle fait, en une phrase",
  "components": {
    "commands": ["commands/salut.md"]
  }
}
```

et le composant qu'il annonce :

```markdown
---
name: salut
description: Dit bonjour
---

Réponds « bonjour » à l'utilisateur.
```

C'est tout. Poussez sur GitHub, l'adresse suffit.

Un exemple complet — permissions, réglages, serveur MCP, agent, règles, hooks
et LSP — vit dans [`examples/plugins/my-plugin`](../examples/plugins/my-plugin).

---

## Ce qu'une extension peut apporter

| Composant | Dossier | Ce que c'est |
|---|---|---|
| **Skills** | `skills/<nom>/SKILL.md` | Un savoir-faire que le modèle mobilise quand la situation s'y prête |
| **Commands** | `commands/<nom>.md` | Une commande `/nom` dans le chat |
| **Agents** | `agents/<nom>.md` | Un sous-agent spécialisé |
| **Rules** | `rules/<nom>.md` | Des consignes permanentes ajoutées au contexte |
| **Hooks** | `hooks/hooks.json` | Du code déclenché sur un événement de l'application |
| **MCP** | `mcp/mcp.json` | Des serveurs MCP, donc des outils réels |
| **LSP** | `lsp/lsp.json` | Un serveur de langage |

Aucun n'est obligatoire. Une extension qui n'apporte qu'une commande est une
extension parfaitement valable.

---

## Modeler l'interface : poser ses boutons, son onglet, ses réglages

Une extension peut décrire ce qu'elle veut voir apparaître à l'écran, sans
qu'aucune ligne de l'application soit écrite pour elle. C'est la seule chose
que Locaryn ajoute aux formats d'ailleurs — les outils en ligne de commande
ne savent pas dire « mets un bouton ici ».

```json
{
  "ui_contributions": {
    "nav_items":     [{ "id": "mon-espace", "label": "Mon espace", "icon": "cube" }],
    "studio_tabs":   [{ "id": "mon-onglet", "label": "Mon onglet", "icon": "star" }],
    "composer_actions": [
      {
        "id": "dicter",
        "label": "Dicter",
        "icon": "mic",
        "action": "tool",
        "value": "transcribe_audio",
        "hint": "Dicter au lieu d'écrire"
      }
    ],
    "settings_sections": [
      {
        "id": "voix",
        "label": "Voix",
        "fields": [
          { "id": "modele", "type": "model", "label": "Modèle de voix" },
          { "id": "debit", "type": "select", "label": "Débit", "options": ["lent", "normal", "rapide"], "default": "normal" }
        ]
      }
    ]
  }
}
```

| Contribution | Où elle apparaît |
|---|---|
| `nav_items` | une entrée du menu principal (ordinateur) |
| `studio_tabs` | un onglet du Studio de génération (ordinateur) |
| `composer_actions` | un bouton à côté du champ de saisie, ordinateur **et** téléphone |
| `settings_sections` | une section des réglages, ordinateur **et** téléphone |

Deux comportements pour un bouton de composeur, pas plus : `insert` écrit
`value` dans le champ, `tool` appelle l'outil nommé avec ce que le champ
contient et met la réponse à la place. L'outil est cherché parmi les serveurs
MCP de toutes les extensions actives — le manifeste nomme un outil, pas un
serveur.

Les champs de réglage ont six types, dessinés à l'identique des deux côtés :
`boolean` (interrupteur), `select` (liste, à partir de `options`), `model`
(liste des modèles installés), `string` (texte), `number` (nombre), `prompt`
(zone multiligne). Les valeurs sont rangées par le serveur, dans le dossier
de l'extension : un réglage choisi sur l'ordinateur vaut sur le téléphone.

`icon` est un nom du jeu partagé `@locaryn/ui-core` — jamais une image. Un
nom inconnu tombe sur une icône de secours, donc vérifiez le vôtre dans la
liste `ICON_NAMES`.

Une extension ne recouvre jamais une entrée native : le menu et les onglets
de l'application restent le socle, vos contributions s'ajoutent à côté. Le
détail du format et son état de mise en œuvre exact sont dans
[`extensions-interop.md`](extensions-interop.md#52-contributions-dinterface).

---

## Permissions : demandez peu, expliquez pourquoi

```json
"permissions": {
  "shell": { "reason": "Lancer les migrations", "scope": "project" },
  "files.read": { "reason": "Lire les sources à relire", "scope": "project" },
  "files.write": {
    "reason": "Appliquer les correctifs proposés",
    "scope": "project",
    "requireApproval": true
  },
  "network": false,
  "env": ["DB_URL"]
}
```

La `reason` est montrée **telle quelle** à l'utilisateur au moment d'accorder
la permission. Ce n'est pas une formalité : c'est le seul endroit où vous
pouvez justifier ce que vous demandez, et une justification vague coûte des
installations.

Ce qui n'est pas demandé n'est pas accessible. Une permission peut être
retirée après coup sans désinstaller l'extension.

---

## Vous avez déjà une extension ailleurs ? Elle marche sans doute

Locaryn lit les manifestes des écosystèmes existants **sans conversion** :

| Fichier détecté | Écosystème |
|---|---|
| `plugin.json` | Locaryn |
| `.claude-plugin/plugin.json` | Claude Code |
| `gemini-extension.json` | Gemini CLI |
| `opencode.json` | OpenCode |

Un greffon Claude Code s'installe donc en collant son adresse, sans rien
réécrire. C'était le but : le format était déjà largement standardisé, et en
inventer un de plus n'aurait servi personne. La table de correspondance
précise — ce qui passe tel quel, ce qui demande un adaptateur, ce qui reste
propre à Locaryn — est dans
[`architecture/09-extension-model.md`](architecture/09-extension-model.md#compatibilité-écosystème--table-de-mapping).

---

## Pendant le développement

Installez depuis un chemin local (`./mon-extension`) plutôt que de pousser à
chaque essai. L'aperçu du manifeste s'affiche de la même façon, ce qui permet
de vérifier ce que verront vos utilisateurs avant de publier quoi que ce soit.

Épinglez une version (`github:vous/extension@v1.0.0`) dès que d'autres
personnes s'en servent : sans cela, chacun installe l'état de votre branche
principale au moment où il clique, ce qui rend tout rapport de bug ambigu.
