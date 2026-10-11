# 17 — Fournisseurs de modèles apportés par un morph

> Un morph peut ajouter un **dossier** dans « Mes modèles » et dans le
> sélecteur du chat. Derrière ce dossier : une API compatible OpenAI — une
> passerelle auto-hébergée, ou un service distant. Ni moteur, ni noyau : rien
> ne calcule de jetons sur cette machine.
>
> L'application ne connaît **aucun** fournisseur en particulier. Tout ce qui
> suit est déclaré par le morph ; n'importe qui peut publier le sien sans
> toucher au code de Locaryn (voir § 8).

---

## 1. Trois choses différentes

| | Noyau (doc 14) | Moteur (doc 15) | Fournisseur (ce document) |
| --- | --- | --- | --- |
| Ce qui change | l'**agent** | **qui calcule** les jetons | **où** les jetons sont calculés |
| Tourne ici | oui | oui | non — ou seulement une passerelle qui route |
| Section du manifeste | `core` | `engine` | `cloud_provider` |
| Choisi par | une session | le fournisseur actif | le modèle actif |

Confondre un fournisseur avec un moteur ferait apparaître une passerelle dans
Réglages → Moteur, à côté de llama.cpp, comme si elle servait des poids. Elle
n'en sert aucun : elle route vers les services chez qui l'utilisateur a un
compte.

---

## 2. Ce que déclare un morph

L'exemple suit une passerelle fictive, `ma-passerelle`, distribuée par npm :

```json
"cloud_provider": {
  "id": "ma-passerelle",
  "label": "Ma passerelle",
  "api_url": "http://127.0.0.1:8899",
  "models_url": "http://127.0.0.1:8899/v1/models",
  "key_required": false,
  "refresh_hours": 1,
  "local": {
    "install": { "kind": "npm", "package": "ma-passerelle", "version": "1.2.3", "probe_bin": "ma-passerelle" },
    "start": ["ma-passerelle", "serve", "--port", "8899"],
    "stop": ["ma-passerelle", "stop"],
    "env": { "HOST": "127.0.0.1", "DATA_DIR": "{{data_dir}}" },
    "secrets": ["JWT_SECRET", "ADMIN_PASSWORD"],
    "dashboard_password": "ADMIN_PASSWORD",
    "provision_key": {
      "command": ["ma-passerelle", "keys", "create", "--name", "Locaryn", "--json"],
      "field": "key"
    },
    "start_timeout_seconds": 180,
    "health_url": "http://127.0.0.1:8899/v1/models",
    "dashboard_url": "http://127.0.0.1:8899/dashboard"
  }
}
```

`api_url` ne porte **pas** `/v1` : la boucle de conversation ajoute
`/v1/chat/completions` elle-même. Le bloc `local` est absent pour un service
purement distant — il n'y a alors rien à démarrer.

`127.0.0.1` et non `localhost` : une passerelle qui n'écoute que la boucle IPv4
reste injoignable si `localhost` se résout d'abord en `::1`.

`key_required` vaut vrai par défaut : un service distant facture, et l'appeler
sans clé ne mène qu'à un refus. Une passerelle qui obtient sa clé toute seule
(`provision_key`) ou converse sans peut le déclarer faux — le choix d'un modèle
n'est alors plus refusé faute de clé, ni par l'application ni par son API.

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

`install.kind` vaut `npm`, `pip`, `docker`, ou `command` (commande explicite).

**Un paquet npm s'installe chez Locaryn**, dans `gateways/<id>` sous le volume
des données lourdes (`locaryn_config::gateways_dir`), avec `npm install
--prefix`. Jamais `-g` : une passerelle Node pèse des centaines de mégaoctets,
et une installation globale la posait sur le disque système, sur le `PATH` de
tout le poste, et hors de portée de la désinstallation du morph. Désinstaller
le morph retire le programme et garde `data/` — les fournisseurs connectés y
restent pour une réinstallation. Une installation interrompue (un
`node_modules` à moitié posé) est retirée avant la suivante.

**Sous Windows**, `npm` est `npm.cmd`, que `Command::new("npm")` ne trouve pas :
l'hôte appelle `npm.cmd`. Et les commandes qui nomment l'exécutable du paquet
lancent directement son script par `node`, lu dans le `bin` de son
`package.json` — aucun raccourci `.cmd`, qui casse sur un chemin à parenthèses.

**Les marqueurs** `{{gateway_dir}}` et `{{data_dir}}` sont remplacés dans les
commandes et les valeurs d'environnement. La version est épinglée par
le manifeste — une chaîne d'approvisionnement sans version installe autre chose
à chaque fois. Un `kind` inconnu ne produit **aucune** commande : l'utilisateur
est renvoyé à `install_hint` plutôt qu'à une approximation exécutée en son nom.

## 3 ter. Ce que l'hôte fait à la place de l'utilisateur

**Les secrets.** Les noms listés dans `secrets` reçoivent chacun une valeur
aléatoire (64 caractères) à la première installation, gardée dans le trousseau
(`locaryn/cloud/<id>/secret/<NOM>`) et passée à chaque démarrage. Une passerelle
publiée avec des secrets par défaut identiques pour tous est ainsi rendue
propre à l'installation. Si le trousseau refuse, la passerelle ne démarre pas :
mieux vaut un refus qu'une passerelle aux secrets publics.

**Le mot de passe du tableau de bord** (`dashboard_password`) est montré à
l'utilisateur sur demande, dans la carte de l'hôte — jamais au panneau du morph.

**La clé.** Une fois la passerelle joignable, si aucune clé n'est enregistrée,
l'hôte lance `provision_key.command` et lit le champ indiqué dans l'objet JSON
imprimé. Une passerelle répond souvent avant que son API de clés soit prête :
l'hôte réessaie plusieurs fois, puis la redemande au plus une fois par minute
tant qu'elle manque.

**L'attente.** `start_timeout_seconds` borne l'attente de la sonde après le
lancement (un premier démarrage peut appliquer des migrations pendant plus
d'une minute) ; la sortie de la passerelle va dans `gateways/<id>/gateway.log`.

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
| La page du dossier | la carte de la passerelle, tenue par l'application (démarrer, arrêter, mot de passe du tableau de bord), puis l'écran du morph (slot `models.folder`). Sans écran déclaré, l'application dessine le sien. |
| Sélecteur du chat | le même dossier, sous le champ de saisie, à côté des modèles de la machine. On l'ouvre, on choisit, la conversation part chez ce modèle. |
| Sous-agents | `lister_modeles` propose les modèles de chaque fournisseur (`<id>:<modèle>`). |

Le tableau de bord d'une passerelle **n'est pas affiché dans un cadre** : une
passerelle peut envoyer `X-Frame-Options: DENY`, et le cadre restait blanc sans
rien dire. Un bouton l'ouvre dans le navigateur du système, avec l'URL du
manifeste — jamais une adresse venue de l'interface.

---

## 5 bis. L'API compatible OpenAI du mode serveur

Locaryn expose son propre dialecte pour son application (`/v1/sessions`…). Ce
qui veut s'y brancher — un éditeur, un agent tiers, un script — parle OpenAI.
Le service ouvre donc les deux routes standard, et **une seule porte pour tout
ce que la machine sait servir** :

| Route | Ce qu'elle rend |
| --- | --- |
| `GET /v1/models` | les poids installés (`owned_by: local`) **et** les modèles de chaque passerelle, préfixés par son identifiant (`ma-passerelle/anthropic/claude-opus-5`) |
| `POST /v1/chat/completions` | la conversation, relayée à qui sert le modèle |

Le serveur résout le modèle demandé : s'il appartient au catalogue d'une
passerelle, la requête part chez elle avec la clé de l'hôte ; sinon elle va au
moteur local actif. Le corps est transmis tel quel et la réponse renvoyée
telle quelle, flux compris. L'`Authorization` du client authentifie auprès de
*ce* serveur et n'est jamais relayée en aval.

Ajouter une passerelle à l'application l'ajoute donc du même coup à son API.

Sur un serveur sans session graphique, il n'y a pas de trousseau : la clé est
lue dans `LOCARYN_CLOUD_<ID>_KEY` (`LOCARYN_CLOUD_MA_PASSERELLE_KEY` pour
l'exemple).

---

## 6. Avec un noyau alternatif

Un modèle de fournisseur fonctionne aussi quand la session est confiée à un
noyau apporté par un autre morph. L'identifiant traverse le pont **tel quel** :
`anthropic/claude-opus-5` arrive entier au noyau, qui route à son tour —
vérifié par `packages/core-bridge/tests/bridge.rs`.

---

## 7. Ce qui est vérifié

| Test | Ce qu'il verrouille |
| --- | --- |
| `packages/extensions` — `une_passerelle_locale_se_lit`, `les_manifestes_des_morphs_presents_se_lisent` | le manifeste, `api_url` sans `/v1`, et pour chaque morph présent : écoute locale, version épinglée, pas d'installation globale, permission `shell` |
| `packages/storage` — `fournisseurs_distants_tests` | un seul fournisseur actif, un modèle distant n'est pas refusé comme « non chargeable », changer de modèle ne multiplie pas les lignes |
| `packages/agent-runtime` — `tests/passerelle.rs` | la route, l'en-tête `Authorization` présent avec clé et **absent** sans, l'identifiant `fournisseur/modèle` intact |
| `packages/core-bridge` — `un_modele_de_passerelle_traverse_le_noyau_tel_quel` | le noyau reçoit l'identifiant du fournisseur |
| `packages/cloud-providers` — `tests/decouverte.rs` | le chemin complet : extension installée → fournisseur découvert → modèle résolu par l'API → choix écrit en base, clé comprise |
| `packages/cloud-providers` — `tests/passerelle_reelle.rs` (manuel) | contre la vraie passerelle d'un morph désigné par `LOCARYN_PASSERELLE_MORPH` : démarrage, clé, catalogue, arrêt |
| `packages/cloud-providers` — `catalog`, `gateway` | lecture du catalogue, fraîcheur, cache non traversant, commandes d'installation déduites et refus des gestionnaires inconnus |

---

## 8. Publier son propre morph de passerelle

Rien à modifier dans Locaryn :

1. Un dépôt avec un `morph.json` : `capabilities: ["cloud-provider"]`, la
   section `cloud_provider` (§ 2), et la permission `shell` si la passerelle
   tourne sur la machine.
2. Facultatif : un écran (`dist/ui.js`, élément personnalisé, slot
   `models.folder`) pour montrer ses modèles à sa façon ; sans lui,
   l'application dessine un dossier générique.
3. Une release GitHub taguée `vX.Y.Z` ; le catalogue lit la version déclarée
   dans le manifeste quand l'API des releases ne répond pas.
4. Vérifier : placer le dossier dans `plugins/` d'un clone de Locaryn et lancer
   `cargo test -p locaryn-extensions les_manifestes_des_morphs_presents_se_lisent`,
   puis `tests/passerelle_reelle.rs` contre la vraie passerelle.

Les particularités d'une passerelle donnée (ses variables d'environnement, ses
secrets par défaut, la durée de son premier démarrage) se documentent dans le
dépôt de son morph, pas ici.
