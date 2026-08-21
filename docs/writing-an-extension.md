# Écrire une extension Locaryn

Ce guide s'adresse à qui veut **fabriquer** une extension. Pour le format exact
et ce que Locaryn accepte des autres écosystèmes — Agent Skills, MCP, plugins
Claude Code — voir [`extensions-interop.md`](extensions-interop.md). Pour la
mécanique interne — cycle de vie, bac à sable, chargement runtime — voir
[`architecture/09-extension-model.md`](architecture/09-extension-model.md).

**Au programme**

- [Le principe : deux produits, deux dépôts](#le-principe--deux-produits-deux-dépôts)
- [Comment quelqu'un installe la vôtre](#comment-quelquun-installe-la-vôtre)
- [L'extension minimale](#lextension-minimale)
- [Ce qu'une extension peut apporter](#ce-quune-extension-peut-apporter)
- [Modeler l'interface](#modeler-linterface--poser-ses-boutons-son-onglet-ses-réglages) — dont [une forme par surface](#une-forme-par-surface) et [hériter du thème](#votre-panneau-hérite-du-thème)
- [Permissions](#permissions--demandez-peu-expliquez-pourquoi)
- [Le pont `window.locaryn`](#le-pont--ce-quun-panneau-peut-demander-à-lapplication)
- [Une extension qui embarque du code](#une-extension-qui-embarque-du-code) — dont [le piège du paquet publié](#le-piège-du-paquet-publié)
- [Apporter ses modèles au catalogue](#apporter-ses-modèles-au-catalogue)
- [Publier, versionner, mettre à jour](#publier-versionner-mettre-à-jour)
- [Quand ça ne marche pas](#quand-ça-ne-marche-pas)

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
| **Interface** | `ui_contributions` du manifeste | Un onglet, un bouton, une section de réglages, un panneau entier |
| **Catalogue** | un slot `marketplace.catalogs` | Des modèles ajoutés au catalogue de l'application |

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

### Une forme par surface

`ui_contributions.slots` est la forme générale : chaque entrée nomme un slot,
un type de rendu et, si besoin, les surfaces qu'elle vise.

```json
"slots": [
  {
    "id": "atelier-large",
    "slot": "studio.tabs",
    "type": "custom-element",
    "entry": "dist/desktop.js",
    "tag": "mon-atelier",
    "platforms": ["desktop"]
  },
  {
    "id": "atelier-compact",
    "slot": "studio.tabs",
    "type": "custom-element",
    "entry": "dist/mobile.js",
    "tag": "mon-atelier-compact",
    "platforms": ["mobile"]
  }
]
```

`platforms` accepte `desktop`, `mobile` et `web`. **Absent ou vide : partout** —
c'est le cas courant, et il ne coûte rien à écrire. Déclarez deux contributions
au même slot quand un écran conçu pour une grande fenêtre n'a pas de sens tel
quel sur un téléphone : vous en donnez une autre forme, ou vous n'en donnez
aucune. L'hôte ne décide pas à votre place, il affiche ce qui vise la surface
où il tourne.

### Votre panneau hérite du thème

Un `custom-element` est monté **dans le document**, sans racine fantôme. Les
classes de l'application s'appliquent donc directement à ce que vous écrivez :
n'emportez pas votre propre feuille de style, servez-vous de la leur et votre
panneau suivra le thème, les couleurs et les espacements sans rien faire.

Le vocabulaire utile : `locaryn-card` et `locaryn-box-card` (blocs),
`locaryn-btn-primary` / `locaryn-btn-ghost` (boutons), `locaryn-chip` et
`locaryn-chip-on` (filtres), `locaryn-input`, `locaryn-select`,
`locaryn-textarea`, `locaryn-tag`, `locaryn-field-hint`. Pour un panneau de
génération, la famille `locaryn-gen-*` donne la mise en page complète :
`locaryn-gen-split` (deux colonnes qui se replient en une sur petit écran),
`locaryn-gen-col`, `locaryn-gen-block`, `locaryn-gen-label`,
`locaryn-gen-tabs`, `locaryn-gen-choices`, `locaryn-gen-canvas`,
`locaryn-gen-thumbs`, `locaryn-gen-error`, `locaryn-gen-lightbox`.

Pour ouvrir un écran de l'application depuis votre panneau — le catalogue de
modèles, les réglages — appelez
`locaryn.ui.dispatchAction("navigate", { view: "models" })` plutôt que de
recopier chez vous ce que l'application sait déjà faire.

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

## Le pont : ce qu'un panneau peut demander à l'application

Un script d'extension reçoit `window.locaryn`. C'est toute la surface : rien
d'autre de l'application n'est accessible depuis un panneau.

```js
const app = window.locaryn;

// Appeler un outil — le vôtre ou celui d'une autre extension active. L'hôte le
// cherche parmi tous les serveurs MCP démarrés : vous nommez un outil, pas un
// serveur.
const res = await app.tools.invoke("generate_image", { prompt: "a red fox" });

// Le champ de saisie et la conversation en cours.
app.chat.getText();
app.chat.insertText("texte ajouté au champ");
app.chat.submit();
await app.chat.appendAssistantMessage("![](…)"); // écrit dans la conversation

// Un chemin de l'hôte, en URL affichable dans une balise img.
const url = app.files.assetUrl("/chemin/vers/image.png");

// Un message court, dans le style de l'application.
app.ui.showToast("Terminé", "success");

// Ouvrir un écran de l'application au lieu de le recopier chez vous.
app.ui.dispatchAction("navigate", { view: "models" });

// Événements entre vos propres composants.
const off = app.events.on("mon-evenement", (data) => { /* … */ });
app.events.emit("mon-evenement", { valeur: 1 });
```

Le pont ne donne accès ni au disque, ni au réseau, ni aux processus. Pour cela
il faut un serveur MCP — du code à vous, hors du navigateur.

---

## Une extension qui embarque du code

Une commande ou une règle sont des fichiers texte. Dès que votre extension doit
lire un disque, lancer un moteur ou télécharger quelque chose, il lui faut un
**serveur MCP** : un programme à vous que l'application démarre et interroge.

```json
// mcp/mcp.json
{
  "mcpServers": {
    "mon-moteur": {
      "command": "${LOCARYN_PLUGIN_ROOT}/bin/mon-serveur",
      "args": [],
      "transport": "stdio",
      "auto_start": true
    }
  }
}
```

L'application injecte dans son environnement des chemins **génériques** — elle
ne sait pas ce que vous en ferez :

| Variable | Ce qu'elle désigne |
|---|---|
| `LOCARYN_PLUGIN_ROOT` | le dossier de votre extension, tel qu'installé |
| `LOCARYN_PLUGIN_BIN_DIR` | son sous-dossier `bin/` |
| `LOCARYN_EXTENSION_DATA_DIR` | un dossier privé, à vous seul |
| `LOCARYN_EXTENSION_MODELS_DIR` | vos poids, dans ce dossier privé |
| `LOCARYN_EXTENSION_MEDIA_DIR` | ce que vous produisez |
| `LOCARYN_MODELS_DIR` | la bibliothèque de poids de l'utilisateur |
| `LOCARYN_DATA_DIR` | la racine de stockage choisie par l'utilisateur |
| `LOCARYN_MODEL_PREFERENCES_FILE` | les préférences de modèles du compte, telles quelles |

Lisez `LOCARYN_MODELS_DIR`. Sans lui, votre extension ne voit que son dossier
privé — vide au premier lancement — et annonce qu'aucun modèle n'est installé
alors que l'utilisateur a déjà tout téléchargé.

### Le piège du paquet publié

**`bin/` est presque toujours dans votre `.gitignore`.** L'archive des sources
d'un dépôt GitHub ne contient donc pas votre binaire, et une extension installée
depuis les sources s'active, s'affiche, et ne fait rien.

L'application cherche pour cette raison **d'abord un paquet de release**, et ne
retombe sur les sources qu'à défaut. Votre CI doit donc compiler par plateforme
et publier une archive nommée avec l'OS et l'architecture :

```
mon-extension-v1.2.0-windows-x86_64.zip
mon-extension-v1.2.0-linux-x86_64.zip
mon-extension-v1.2.0-macos-aarch64.zip
```

L'archive contient ce qui tourne chez l'utilisateur, sans vos sources :
`plugin.json`, `bin/`, `dist/`, `mcp/`, `SKILL.md`, `README`, `LICENSE`.
Vérifiez la présence de `bin/` dans l'archive avant de publier — c'est une ligne
de CI, et elle vous épargne une version installable mais inerte.

Un paquet destiné à une autre plateforme n'est jamais retenu : mieux vaut
retomber sur les sources que d'installer un binaire qui ne démarrera pas.

---

## Apporter ses modèles au catalogue

Ne dressez pas la liste de vos modèles dans votre propre panneau : elle se fige
à la version du paquet, et les adresses qu'elle contient finissent par répondre
404 ou 401. Déclarez un **slot de données**, et vos modèles apparaissent dans le
catalogue de l'application, avec les autres.

```json
{
  "id": "mon-catalogue",
  "slot": "marketplace.catalogs",
  "type": "data",
  "entry": "dist/marketplace.json"
}
```

```json
// dist/marketplace.json
{
  "schemaVersion": 1,
  "refreshUrl": "https://raw.githubusercontent.com/vous/extension/main/dist/marketplace.json",
  "owns": ["mon-prefixe", "autre-motif"],
  "categories": [
    {
      "id": "ma-categorie",
      "label": "Ma catégorie",
      "icon": "image",
      "matches": ["ma-capacite"],
      "requires": ["ma-capacite"]
    }
  ],
  "models": [
    {
      "id": "mon-modele",
      "name": "Mon modèle",
      "brand": "Auteur",
      "description": "Ce qu'il fait, et pour qui.",
      "license": "Apache-2.0",
      "releaseDate": "2026-01-15",
      "releaseYear": 2026,
      "capabilities": ["ma-capacite"],
      "variants": [
        {
          "size": "Q4_K · 6B",
          "params": 6,
          "storageGb": 6.2,
          "quants": ["q4_K"],
          "tag": "https://huggingface.co/…/poids.gguf",
          "downloads": [
            {
              "url": "https://huggingface.co/…/vae.safetensors",
              "file": "vae.safetensors",
              "label": "VAE"
            }
          ]
        }
      ]
    }
  ]
}
```

Ce qu'il faut retenir :

- **`refreshUrl`** est relue au lancement. Votre catalogue continue d'évoluer
  après la publication du paquet, sans réinstallation. Hors-ligne, l'application
  reprend la dernière copie valide, puis le fichier livré — jamais rien de moins.
- **`downloads`** liste les fichiers compagnons que votre moteur exige (VAE,
  encodeurs). L'application les installe avec le modèle. Un modèle livré sans
  ses compagnons s'installe, puis échoue au premier usage.
- **`owns`** revendique les poids **déjà** présents sur le disque : des
  fragments de nom, en minuscules. C'est ce qui fait apparaître dans « Mes
  modèles installés » ce que l'utilisateur avait téléchargé avant votre
  extension, en disant qui s'en sert.
- **`requires`** ne montre votre filtre que si la capacité correspondante est
  active : votre catégorie disparaît proprement quand votre extension est
  désactivée.
- **Vérifiez chaque adresse** avant de publier. Un dépôt privé ou sous licence à
  accepter répond 401, et l'installation échoue chez l'utilisateur, pas chez
  vous. Une vérification en CI coûte dix lignes.

---

## Publier, versionner, mettre à jour

1. La version du manifeste fait foi. Montez-la à chaque publication.
2. Posez un tag `vX.Y.Z` ; votre CI construit et publie les archives.
3. L'application compare la version installée à celle de votre branche
   principale et propose la mise à jour.
4. Une mise à jour **conserve** l'identité de l'extension, son état actif et les
   permissions déjà accordées. Une permission que votre manifeste cesse de
   demander est retirée ; une nouvelle est soumise à l'utilisateur.

---

## Quand ça ne marche pas

Trois causes couvrent l'essentiel des extensions « installées mais inertes » :

| Symptôme | Cause probable | Vérification |
|---|---|---|
| Panneau vide, aucun outil | permission `mcp` jamais accordée | Réglages → Extensions : la fiche nomme la permission manquante |
| Serveur qui ne démarre pas | `bin/` absent du paquet installé | ouvrez le dossier de l'extension et cherchez votre binaire |
| Modèles introuvables | vous ne lisez que votre dossier privé | lisez aussi `LOCARYN_MODELS_DIR` |

L'appel d'outil qui échoue nomme la cause : l'application ne répond pas
« indisponible », elle dit ce qui manque et où le corriger.

---

## Une extension complète, à lire

[`plugin-image`](https://github.com/Locaryn/plugin-image) met tout cela
en œuvre : un serveur MCP compilé et publié par plateforme, un panneau de Studio
rendu avec les classes de l'application, un catalogue de modèles qui se
rafraîchit, une compétence qui explique au modèle comment appeler l'outil. C'est
l'exemple de référence quand ce guide reste abstrait.

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
