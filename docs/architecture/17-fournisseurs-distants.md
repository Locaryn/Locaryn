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
  "api_url": "http://127.0.0.1:20128",
  "models_url": "http://127.0.0.1:20128/v1/models",
  "key_required": false,
  "refresh_hours": 1,
  "local": {
    "install": { "kind": "npm", "package": "omniroute", "version": "3.8.50", "probe_bin": "omniroute" },
    "start": ["omniroute", "serve", "--no-open", "--port", "20128"],
    "stop": ["omniroute", "stop"],
    "env": { "OMNIROUTE_SERVER_HOST": "127.0.0.1", "DATA_DIR": "{{data_dir}}" },
    "secrets": ["JWT_SECRET", "API_KEY_SECRET", "INITIAL_PASSWORD"],
    "dashboard_password": "INITIAL_PASSWORD",
    "provision_key": {
      "command": ["omniroute", "api", "api-keys", "post-api-keys", "--body", "{\"name\":\"Locaryn\"}", "--output", "json"],
      "field": "key"
    },
    "start_timeout_seconds": 180,
    "health_url": "http://127.0.0.1:20128/v1/models",
    "dashboard_url": "http://127.0.0.1:20128/dashboard"
  }
}
```

`api_url` ne porte **pas** `/v1` : la boucle de conversation ajoute
`/v1/chat/completions` elle-même. Le bloc `local` est absent pour un service
purement distant — il n'y a alors rien à démarrer.

`127.0.0.1` et non `localhost` : la passerelle n'écoute que la boucle IPv4, et
`localhost` peut se résoudre d'abord en `::1`.

`key_required` vaut vrai par défaut : un service distant facture, et l'appeler
sans clé ne mène qu'à un refus. Une passerelle qui route vers des modèles
gratuits dès son installation le déclare faux — le choix d'un modèle n'est alors
plus refusé faute de clé, ni par l'application ni par son API.

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
  "install": { "kind": "npm", "package": "omniroute", "version": "3.8.50", "probe_bin": "omniroute" }
}
```

`npm`, `pip`, `docker`, ou une `command` explicite.

**Un paquet npm s'installe chez Locaryn**, dans `gateways/<id>` sous le volume
des données lourdes (`locaryn_config::gateways_dir`), avec `npm install
--prefix`. Jamais `-g` : OmniRoute pèse 450 Mo, et une installation globale le
posait sur le disque système, sur le `PATH` de tout le poste, et hors de portée
de la désinstallation du morph. Désinstaller le morph retire le programme et
garde `data/` — les fournisseurs connectés y restent pour une réinstallation.

**Sous Windows**, `npm` est `npm.cmd`, que `Command::new("npm")` ne trouve pas :
l'hôte appelle `npm.cmd`. Et les commandes qui nomment l'exécutable du paquet
(`omniroute …`) lancent directement son script par `node`, lu dans le `bin` de
son `package.json` — aucun raccourci `.cmd` à trouver.

**Les marqueurs** `{{gateway_dir}}` et `{{data_dir}}` sont remplacés dans les
commandes et les valeurs d'environnement. La version est épinglée par
le manifeste — une chaîne d'approvisionnement sans version installe autre chose
à chaque fois. Un `kind` inconnu ne produit **aucune** commande : l'utilisateur
est renvoyé à `install_hint` plutôt qu'à une approximation exécutée en son nom.

## 3 ter. Ce que l'hôte fait à la place de l'utilisateur

**Les secrets.** Les noms listés dans `secrets` reçoivent chacun une valeur
aléatoire à la première installation, gardée dans le trousseau
(`locaryn/cloud/<id>/secret/<NOM>`) et passée à chaque démarrage. Le paquet npm
d'OmniRoute embarque un `.env` au même `JWT_SECRET` pour toutes les
installations et le mot de passe `CHANGEME` ; les valeurs de l'environnement
priment sur ce fichier. Si le trousseau refuse, la passerelle ne démarre pas :
mieux vaut un refus qu'une passerelle aux secrets publics.

**Le mot de passe du tableau de bord** (`dashboard_password`) est montré à
l'utilisateur sur demande, dans la carte de l'hôte — jamais au panneau du morph.

**La clé.** Une fois la passerelle joignable, si aucune clé n'est enregistrée,
l'hôte lance `provision_key.command` et lit le champ indiqué dans l'objet JSON
imprimé. Pour OmniRoute, la ligne de commande locale s'authentifie par un jeton
dérivé de la machine et crée la clé sans mot de passe. Sans clé, `/v1/models`
répond 401 alors que la conversation passe.

**L'écoute.** Par défaut OmniRoute ouvre toutes les interfaces sans clé : tout
le réseau peut dépenser les quotas de l'utilisateur. Le manifeste impose
`OMNIROUTE_SERVER_HOST=127.0.0.1`.

**L'attente.** `start_timeout_seconds` borne l'attente de la sonde après le
lancement. Le premier démarrage d'OmniRoute applique 159 migrations, bien
au-delà d'une minute ; la sortie de la passerelle va dans
`gateways/<id>/gateway.log`.

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
| La page du dossier | la carte de la passerelle, tenue par l'application (démarrer, arrêter, mot de passe du tableau de bord), puis l'écran du morph (slot `models.folder`) — pour OmniRoute, ses modèles et sa clé. Sans écran déclaré, l'application dessine le sien. |
| Sélecteur du chat | le même dossier, sous le champ de saisie. On l'ouvre, on choisit, la conversation part chez ce modèle. |

Le tableau de bord d'une passerelle **n'est pas affiché dans un cadre** :
OmniRoute envoie `X-Frame-Options: DENY` et `frame-ancestors 'none'` sur toutes
ses pages, et le cadre restait blanc sans rien dire. Un bouton l'ouvre dans le
navigateur du système, avec l'URL du manifeste — jamais une adresse venue de
l'interface.

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
