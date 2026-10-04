# Le schéma `locaryn://` — liens d'installation et de connexion

Ce document dit **exactement** ce que l'application accepte comme lien
`locaryn://`, et ce qu'elle en fait. Il est écrit pour qui fabrique un
générateur de liens : le morph Remote qui produit le `.exe` d'appairage et le
QR du téléphone, mais aussi tout outil qui veut ouvrir Locaryn sur une machine
existante.

Le code qui fait vivre ce contrat : le parseur
[`apps/desktop/src/lib/deepLink.ts`](../../apps/desktop/src/lib/deepLink.ts),
le dispatch dans [`App.tsx`](../../apps/desktop/src/App.tsx), la modale de
consentement
[`ConnectIntentModal.tsx`](../../apps/desktop/src/components/ConnectIntentModal.tsx),
et l'enregistrement du schéma côté Rust
(`apps/desktop/src-tauri/src/lib.rs`, plugin `tauri-plugin-deep-link`).

---

## 1. Vue d'ensemble

| Lien | Porté par | Effet dans l'application |
| --- | --- | --- |
| `locaryn://install?src=…` | catalogues, pages web | ouvre Réglages → Extensions et pré-remplit la fenêtre d'installation |
| `locaryn://connect?…` | le `.exe` du morph Remote, le QR du téléphone | pop-up de consentement, puis connexion au serveur (certificats installés d'abord) |

Le schéma est déclaré dans `tauri.conf.json` (`plugins.deep-link`) et
enregistré à chaque lancement. Un lien peut arriver de deux façons, avec le
même traitement :

- **À froid** — le lien a ouvert l'application : l'URL arrive en argument de
  ligne de commande, relue par le plugin au démarrage.
- **À chaud** — l'application tourne déjà : le plugin émet
  `deep-link://new-url`, ré-émis côté Rust en `locaryn://deep-link`.

En mode démo navigateur (sans Tauri), l'équivalent est une ancre :
`#locaryn://connect?…`.

## 2. Règles communes

- L'**action** est l'hôte de l'URL : `locaryn://connect?…`, pas
  `locaryn:///connect?…` (les deux formes sont acceptées, la première est la
  norme).
- Tous les paramètres passent dans la query, **encodés** (`encodeURIComponent`
  côté générateur). L'URL du serveur contient `:` et `/` — toujours encodés.
- Une action inconnue, ou des paramètres invalides, sont **ignorés sans
  erreur** : un lien ne fait jamais rien sans consentement (voir §4).
- Une action refusée par l'utilisateur ne laisse **aucune trace** : rien
  n'est installé, rien n'est stocké, rien ne se connecte.

## 3. Action `install`

```
locaryn://install?src=<source>
```

| Paramètre | Requis | Valeur |
| --- | --- | --- |
| `src` | oui | dépôt GitHub (`owner/repo`), URL `https://…` d'une archive, ou chemin local |

Effet : ouvre le panneau Réglages et pré-remplit la fenêtre d'installation
avec la source. L'utilisateur relit et confirme — le lien ne suffit jamais à
installer.

## 4. Action `connect`

```
locaryn://connect?server=<url>&user=<identifiant>&password=<mot-de-passe>&cert=<url>&ca=<url>
```

| Paramètre | Requis | Valeur |
| --- | --- | --- |
| `server` | **oui** | URL de base du serveur, `https://…` ou `http://…`, utilisée telle quelle pour la connexion. Le parseur accepte les deux ; préférez `https://` — un serveur en clair ne devrait exister qu'en réseau local de confiance. En revanche les URLs `cert`/`ca` sont **HTTPS strict** (voir §4.2). |
| `user` | non | identifiant pré-rempli. Absent : demandé dans la modale. |
| `password` | non | mot de passe pré-enregistré **par l'utilisateur lui-même** dans le lien (case du morph Remote). Absent : demandé dans la modale. Jamais affiché, jamais replacé ailleurs. |
| `cert` | non | URL **HTTPS** du paquet certificat client + clé (PEM), téléchargé et installé avant la connexion. |
| `ca` | non | URL **HTTPS** du certificat d'autorité, quand le serveur n'utilise pas une autorité publique. |

### 4.1 Ce que fait l'application

1. La modale de consentement s'affiche — **toujours**, y compris sur une
   application pas encore connectée (elle est montée au-dessus du gate
   d'authentification). Elle montre le serveur, le compte si fourni, et
   mentionne l'installation de certificat.
2. Si le lien porte un mot de passe, la modale le dit et prévient :
   « Ne transmettez un tel lien qu'à vos propres machines. »
3. **Accepter** : les certificats d'abord (`cert` puis `ca`, voir §4.2), puis
   connexion `POST /v1/auth/login` contre `server` avec identifiant et mot de
   passe, puis la session est enregistrée comme une connexion classique —
   l'hôte entre du même coup dans l'historique de
   Réglages → Connexion.
4. **Refuser**, Échap, clic dehors : le lien est oublié, rien ne se passe.

Les champs identifiant / mot de passe sont demandés dans la modale quand le
lien ne les porte pas. Un mot de passe dans un lien est un confort que
l'utilisateur s'accorde à lui-même — jamais un mécanisme par défaut.

### 4.2 Les certificats : ordre et limites

- Le téléchargement est **HTTPS strict** : la commande Rust refuse toute URL
  `cert`/`ca` en `http://`, et la connexion TLS du téléchargement est validée
  normalement. **Conséquence pour le générateur** : l'URL qui sert les
  certificats doit présenter un certificat TLS de confiance publique
  (reverse proxy, tunnel). Un serveur en autorité auto-signée ne peut pas
  servir ses propres certificats par ce chemin — dans ce cas l'utilisateur
  installe à la main (Réglages → Connexion → Installer…), ce que le message
  d'erreur indique.
- En cas d'échec du téléchargement, **la connexion n'a pas lieu** : mieux vaut
  pas de connexion qu'une connexion qui croit être sûre sans l'être.
- Les certificats sont enregistrés dans le répertoire de données de
  l'application (pas dans le magasin Windows) — le même endroit que
  l'installation manuelle.

### 4.3 Exemples

(Les retours à la ligne ci-dessous ne sont là que pour la lecture — un vrai
lien tient sur une seule ligne, sans espace.)

Minimum — l'application demandera identifiant et mot de passe :

```
locaryn://connect?server=https%3A%2F%2F192.168.1.10%3A7474
```

Complet — certificats hébergés par le serveur, identifiant pré-rempli :

```
locaryn://connect?server=https%3A%2F%2Flocaryn.example.net%3A7474
  &user=dev
  &cert=https%3A%2F%2Flocaryn.example.net%2Fpair%2Fa1b2c3%2Fclient.pem
  &ca=https%3A%2F%2Flocaryn.example.net%2Fpair%2Fa1b2c3%2Fca.pem
```

## 5. Ce que le morph Remote doit générer

Le `.exe` d'appairage est un lanceur minuscule : il porte le lien et l'ouvre.
Rien de plus — la logique de connexion vit dans l'application, jamais dans le
fichier.

1. **Construire le lien** côté serveur : le morph connaît l'URL publique du
   serveur, les URLs des certificats qu'il héberge, et l'identifiant de
   l'utilisateur. Encoder chaque paramètre (`encodeURIComponent`).
2. **Le mot de passe reste un choix explicite** : une case « inclure le mot
   de passe dans le fichier », **décochée par défaut**. Quand elle est cochée,
   le paramètre `password` est embarqué en clair dans le fichier — l'utilisateur
   est prévenu que ce fichier ne se partage pas.
3. **Ouvrir le lien** : sur Windows, exécuter `open("locaryn://connect?…")`
   (un `.url` ou un appel `ShellExecute` font l'affaire) — le schéma est
   enregistré par l'application à l'installation et à chaque lancement. Si
   Locaryn n'est pas installé, l'ouverture échoue silencieusement : proposer
   alors un lien de téléchargement dans l'interface du morph.
4. **Le QR du téléphone** porte le même lien ; l'application mobile lit le
   même schéma.
5. **Bonnes pratiques côté serveur** : servir les certificats par des chemins
   aléatoires à durée de vie courte, révoqués après installation ; ne régénérer
   le `.exe` qu'à la demande de l'utilisateur.

## 6. Limites connues

- Un lien ne porte ni nonce ni signature : la borne de sécurité est la modale
  de consentement. Ne jamais mettre dans un lien un secret que l'utilisateur
  ne possède pas.
- Le mot de passe d'un lien vit en clair dans le fichier qui le porte — c'est
  pourquoi il est opt-in et averti.
- La bascule « full local ↔ serveur » et l'historique des serveurs vivent dans
  Réglages → Connexion. Une action `locaryn://disconnect` est envisageable un
  jour ; elle n'existe pas encore.
