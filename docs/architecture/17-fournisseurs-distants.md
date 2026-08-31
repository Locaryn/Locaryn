# 17 — Fournisseurs de modèles apportés par un morph

> Un morph peut ajouter un **dossier** dans « Mes modèles » et dans le
> sélecteur du chat. Derrière ce dossier : une API compatible OpenAI — une
> passerelle auto-hébergée comme OmniRoute, ou un service distant. Ni moteur,
> ni noyau : rien ne calcule de jetons sur cette machine.

---

## 1. Trois choses différentes

| | Noyau (doc 14) | Moteur (doc 15) | Fournisseur (ce document) |
| --- | --- | --- | --- |
| Ce qui change | l'**agent** | **qui calcule** les jetons | **où** les jetons sont calculés |
| Tourne ici | oui | oui | non — ou seulement une passerelle qui route |
| Section du manifeste | `core` | `engine` | `cloud_provider` |
| Choisi par | une session | le fournisseur actif | le modèle actif |

Confondre un fournisseur avec un moteur ferait apparaître OmniRoute dans
Réglages → Moteur, à côté de llama.cpp, comme s'il servait des poids. Il n'en
sert aucun : il route vers Anthropic, OpenAI, Google — chez qui l'utilisateur
a un compte.

---

## 2. Ce que déclare un morph

```json
"cloud_provider": {
  "id": "omniroute",
  "label": "OmniRoute",
  "api_url": "http://localhost:20128",
  "models_url": "http://localhost:20128/v1/models",
  "keys_url": "http://localhost:20128",
  "refresh_hours": 1,
  "local": {
    "start": ["omniroute"],
    "health_url": "http://localhost:20128/v1/models",
    "dashboard_url": "http://localhost:20128",
    "install_hint": "npm install -g omniroute"
  }
}
```

`api_url` ne porte **pas** `/v1` : la boucle de conversation ajoute
`/v1/chat/completions` elle-même. Le bloc `local` est absent pour un service
purement distant — il n'y a alors rien à démarrer.

---

## 3. Ce que l'hôte garde pour lui

Trois choses, et aucune ne peut vivre dans l'extension.

**La clé** va dans le trousseau du système, sous `locaryn/cloud/<id>`. Le
panneau du morph peut demander à l'écrire et savoir qu'elle existe ; il ne peut
pas la relire. Une extension compromise ne rend donc pas la clé de son
utilisateur. C'est l'hôte qui l'ajoute aux requêtes, au moment de parler au
modèle.

**La commande de démarrage** vient du manifeste et de nulle part ailleurs, et
exige la permission `shell`. Ni l'interface ni le panneau ne peuvent en
proposer une autre.

**Le choix du modèle** s'écrit comme fournisseur actif : `kind = remote`,
moteur `open_ai_compat`, et un marqueur `config.cloud_provider = <id>`. C'est ce
marqueur que la conversation relit pour joindre la bonne clé.

---

## 3 bis. Installer la passerelle avec le morph

Un morph qui apporte une passerelle déclare comment l'installer :

```json
"local": {
  "install": { "kind": "npm", "package": "omniroute", "probe_bin": "omniroute" }
}
```

`npm`, `pip`, `docker`, ou une `command` explicite. La version est épinglée par
le manifeste — une chaîne d'approvisionnement sans version installe autre chose
à chaque fois. Un `kind` inconnu ne produit **aucune** commande : l'utilisateur
est renvoyé à `install_hint` plutôt qu'à une approximation exécutée en son nom.

**Activer le morph installe la passerelle** : la permission `shell` accordée,
l'application lance l'installation en tâche de fond, puis démarre la
passerelle. En cas d'échec — pas de Node, pas de réseau — l'activation
n'échoue pas pour autant : le dossier du fournisseur dit ce qui manque et
propose de recommencer. Démarrer une passerelle absente l'installe d'abord :
c'est un enchaînement qui n'a qu'une issue.

---

## 4. Le catalogue se tient à jour tout seul

La liste des modèles est lue chez le fournisseur (`GET {models_url}`), jamais
figée dans le paquet : un modèle publié ce matin apparaît sans nouvelle version
du morph. Elle est gardée sur disque (`<données>/cloud/<id>.json`) pendant
`refresh_hours`, et resservie telle quelle quand la lecture échoue — une liste
d'hier vaut mieux qu'un écran vide.

Les trois formes de réponse rencontrées se lisent : `{ "data": [...] }`,
`{ "models": [...] }`, ou un tableau nu.

---

## 5. Où ça se voit

| Endroit | Ce qui apparaît |
| --- | --- |
| Mes modèles | un **dossier**, à la place d'une carte de modèle. Il dit si la clé est posée, combien de modèles sont routés, lequel est actif. |
| La page du dossier | l'écran du morph (slot `models.folder`) — pour OmniRoute, son tableau de bord embarqué. Sans écran déclaré, l'application dessine le sien. |
| Sélecteur du chat | le même dossier, sous le champ de saisie. On l'ouvre, on choisit, la conversation part chez ce modèle. |

Le tableau de bord d'une passerelle locale est affiché dans un cadre : la
politique de sécurité de l'application autorise les cadres de la **boucle
locale** uniquement (`frame-src 'self' http://localhost:* http://127.0.0.1:*`).
Un bouton ouvre la même page dans le navigateur du système, avec l'URL du
manifeste — jamais une adresse venue de l'interface.

---

## 5 bis. L'API compatible OpenAI du mode serveur

Locaryn expose son propre dialecte pour son application (`/v1/sessions`…). Ce
qui veut s'y brancher — un éditeur, un agent tiers, un script — parle OpenAI.
Le service ouvre donc les deux routes standard, et **une seule porte pour tout
ce que la machine sait servir** :

| Route | Ce qu'elle rend |
| --- | --- |
| `GET /v1/models` | les poids installés (`owned_by: local`) **et** les modèles de chaque passerelle, préfixés par son identifiant (`omniroute/anthropic/claude-opus-5`) |
| `POST /v1/chat/completions` | la conversation, relayée à qui sert le modèle |

Le serveur résout le modèle demandé : s'il appartient au catalogue d'une
passerelle, la requête part chez elle avec la clé de l'hôte ; sinon elle va au
moteur local actif. Le corps est transmis tel quel et la réponse renvoyée
telle quelle, flux compris — réécrire l'un ou l'autre ferait perdre les champs
que ce serveur ne connaît pas encore. L'`Authorization` du client authentifie
auprès de *ce* serveur et n'est jamais relayée en aval.

Ajouter OmniRoute à l'application l'ajoute donc du même coup à son API : un
client tiers pointé sur Locaryn voit les modèles locaux et les modèles routés
dans la même liste, et les appelle de la même façon.

Sur un serveur sans session graphique, il n'y a pas de trousseau : la clé est
lue dans `LOCARYN_CLOUD_<ID>_KEY` — `LOCARYN_CLOUD_OMNIROUTE_KEY` pour
OmniRoute.

---

## 6. Avec un noyau alternatif

Un modèle de fournisseur fonctionne aussi quand la session est confiée à un
noyau apporté par un autre morph. L'identifiant traverse le pont **tel quel** :
`anthropic/claude-opus-5` arrive entier au noyau, qui route à son tour. Le
réécrire ou retomber sur le modèle par défaut du manifeste enverrait la
conversation ailleurs que là où l'utilisateur l'a envoyée — c'est vérifié par
`packages/core-bridge/tests/bridge.rs`.

---

## 7. Ce qui est vérifié

| Test | Ce qu'il verrouille |
| --- | --- |
| `packages/extensions` — `une_passerelle_locale_se_lit` | le manifeste, et `api_url` sans `/v1` |
| `packages/storage` — `fournisseurs_distants_tests` | un seul fournisseur actif, un modèle distant n'est pas refusé comme « non chargeable », changer de modèle ne multiplie pas les lignes |
| `packages/agent-runtime` — `tests/passerelle.rs` | la route, l'en-tête `Authorization` présent avec clé et **absent** sans, l'identifiant `fournisseur/modèle` intact |
| `packages/core-bridge` — `un_modele_de_passerelle_traverse_le_noyau_tel_quel` | le noyau reçoit l'identifiant du fournisseur |
| `packages/cloud-providers` — `tests/decouverte.rs` | le chemin complet : extension installée → fournisseur découvert → modèle résolu par l'API → choix écrit en base, clé comprise |
| `packages/cloud-providers` — `catalog`, `gateway` | lecture du catalogue, fraîcheur, cache non traversant, commandes d'installation déduites et refus des gestionnaires inconnus |
