# 12 — Mode serveur, sécurité et déploiement

Ce document décrit comment Locaryn passe d'un usage personnel à un service
partagé, et pourquoi les choix de sécurité sont ce qu'ils sont.

---

## 1. Deux produits, un seul cœur

| Produit | Contenu | Public |
| --- | --- | --- |
| **locaryn-server** | `locaryn-daemon` + `locaryn` (CLI). **Aucune dépendance graphique** | Machine à GPU, sans session de bureau |
| **locaryn** | Application desktop, qui fait **aussi** serveur | Poste de travail |

La séparation est vérifiable : `cargo tree -p locaryn-daemon` ne contient ni
Tauri ni WebKit, et les binaires de release pèsent 6,8 Mo et 3,8 Mo. Avec un
moteur de rendu embarqué on dépasserait 100 Mo.

Les deux exposent **la même API HTTP**. Le CLI, le futur client mobile et
l'application desktop en mode client parlent tous à cette interface — il n'y a
pas trois implémentations à maintenir.

---

## 2. La sécurité suit l'adresse d'écoute

C'est la règle centrale, et elle n'est pas configurable :

| Écoute sur | Authentification | Chiffrement |
| --- | --- | --- |
| `127.0.0.1` | non requise | non (le trafic ne quitte pas la machine) |
| toute autre adresse | **obligatoire** | **obligatoire (TLS)** |

Sur la boucle locale, le seul appelant est la personne au clavier : le système
d'exploitation l'a déjà authentifiée, et exiger un jeton serait de la friction
sans bénéfice.

Dès que le daemon devient joignable depuis le réseau, chaque requête porte un
jeton — et un jeton qui circule en clair est lisible par quiconque se trouve sur
le chemin. Les deux protections vont donc ensemble.

**Pourquoi lier ça à l'adresse plutôt qu'à un réglage :** un serveur exposé sans
protection ne doit pas pouvoir exister parce qu'une case a été oubliée. Le
daemon refuse d'ailleurs de démarrer s'il est exposé sans aucun compte :

```
Error: Le daemon est configuré pour écouter sur 0.0.0.0 mais aucun compte n'existe.
Un serveur accessible sans compte serait ouvert à tous, donc il ne démarre pas.
```

### Configuration

| Variable | Fichier | Rôle |
| --- | --- | --- |
| `LOCARYN_DAEMON_BIND` | `daemon.bind` | Adresse d'écoute — décide de la posture |
| `LOCARYN_DAEMON_PORT` | `daemon.port` | Port (7474 par défaut) |
| `LOCARYN_DATA_DIR` | `daemon.data_dir` | Base et certificats |
| `LOCARYN_TLS_CERT` / `LOCARYN_TLS_KEY` | `daemon.tls_cert` / `tls_key` | Certificat fourni |

---

## 3. Comptes et jetons

Table `users` (Argon2id) et `auth_tokens` (empreinte Argon2id du jeton).

```bash
locaryn users add patron --admin     # mot de passe lu sur l'entrée standard
locaryn users list
locaryn users disable marie          # ses jetons cessent de fonctionner aussitôt
locaryn users enable marie
```

`locaryn users` travaille **directement sur la base**, pas via le daemon : il
faut pouvoir créer le premier administrateur avant que le service démarre.

Le mot de passe est lu sur stdin et jamais accepté en argument — un mot de passe
sur la ligne de commande atterrit dans l'historique du shell et dans la liste des
processus.

### Propriétés garanties par des tests

- Le mot de passe n'est **jamais** stocké en clair ; l'empreinte commence par
  `$argon2id$`.
- Un mauvais mot de passe, un compte inconnu et un compte désactivé produisent
  **la même réponse**. Les distinguer permettrait d'énumérer les comptes.
  Un hachage factice est calculé même quand l'utilisateur n'existe pas, pour que
  le temps de réponse ne trahisse rien.
- Les noms sont comparés **sans tenir compte de la casse** : « Marie » et
  « marie » ne peuvent pas être deux comptes.
- Un jeton révoqué, ou dont le compte est désactivé, cesse d'être accepté
  immédiatement.

### Points d'entrée publics

Seuls `/health` et `/v1/auth/login` sont accessibles sans jeton. Tout le reste
répond 401.

```bash
curl -k -X POST https://serveur:7474/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"patron","password":"…"}'
# → { "token": "locaryn_…", "expires_at": "…", "user": { … } }
```

---

## 4. TLS

Deux façons d'obtenir un certificat :

- **Fourni** — `tls_cert` et `tls_key` pointent vers des fichiers PEM. C'est ce
  qu'utilisera une entreprise disposant de sa propre autorité.
- **Généré** — sans certificat, une paire auto-signée est créée au premier
  démarrage exposé, puis réutilisée. Elle couvre `localhost`, l'adresse IP
  locale et le nom de la machine : un certificat valable seulement pour
  `localhost` serait rejeté dès qu'un client se connecte par IP.

**Un certificat nommé mais introuvable est une erreur, jamais un repli en clair.**
C'est ainsi qu'on croit être chiffré sans l'être.

Le certificat n'est pas régénéré à chaque démarrage : sinon l'empreinte
changerait sans cesse et la décision de confiance n'aurait plus de sens.

### L'empreinte

Affichée au démarrage :

```
certificat auto-signé généré pour localhost, 127.0.0.1, SERVEUR, 192.168.1.188 —
les clients afficheront un avertissement d'émetteur inconnu au premier contact,
c'est attendu. Empreinte à comparer : BD:E9:FA:13:…
```

Un certificat auto-signé empêche l'écoute passive. Il ne prouve pas **quelle**
machine a répondu — d'où l'empreinte. Sans elle, un client n'a que deux
options : refuser toute connexion, ou accepter n'importe laquelle. C'est la
seconde qui rend une interception triviale.

---

## 5. Déploiement en entreprise

L'objectif : un employé ne doit **jamais** saisir une adresse de serveur, un
port ou une empreinte. Cela ressemble à de l'administration réseau, et c'est là
que les déploiements échouent.

### Côté administrateur, une fois

```bash
locaryn provision 192.168.1.188 --org "Atelier Durand" \
  --note "Identifiants fournis par le service informatique"
```

Produit `locaryn-connect.json` :

```json
{
  "serverUrl": "https://192.168.1.188:7474",
  "organisation": "Atelier Durand",
  "certificateFingerprint": "BD:E9:FA:13:…",
  "note": "Identifiants fournis par le service informatique"
}
```

L'adresse est normalisée : `192.168.1.188`, `192.168.1.188:7474` et une URL
complète donnent le même résultat, avec HTTPS par défaut puisqu'un daemon
exposé sert toujours en TLS.

### Distribution

Poser le fichier **à côté du `.msi`**, ou dans `C:\ProgramData\Locaryn\`.
Le client le cherche dans cet ordre : répertoire de l'exécutable,
`%PROGRAMDATA%\Locaryn`, puis `~/.locaryn`.

### Côté employé

Lancer l'installeur, ouvrir l'application, saisir ses identifiants. C'est tout.

### Pourquoi pas un `.msi` reconstruit par le serveur

Ce serait la solution intuitive, et elle coûte cher : il faudrait Rust, Node,
pnpm et WiX sur la machine de production, plus plusieurs minutes de compilation
à chaque changement d'adresse.

Le fichier de configuration atteint le même résultat avec trois avantages :
le **même installeur officiel** sert partout, l'adresse change sans rien
recompiler, et le fichier **ne contient aucun secret** — il indique où est le
serveur et quel certificat attendre, il n'accorde rien. Il peut circuler par
courriel sans précaution particulière.

---

## 6. Le CLI

```bash
locaryn                    # l'agent, dans le dossier courant — il lit et modifie vos fichiers
locaryn chat               # conversation simple, aucun accès aux fichiers
```

La distinction ne vient pas d'un drapeau : le moteur active sa boucle d'outils
**uniquement** quand il reçoit un espace de travail réel. `locaryn` rattache la
session au projet du dossier courant ; `locaryn chat` la rattache au conteneur
« conversations libres », qui n'a pas de dossier.

Le daemon vérifie que le chemin du projet est un **répertoire existant** avant
de le transmettre — sinon l'agent recevrait un espace de travail illisible et
chaque outil de fichier échouerait.

Dans les deux modes, le raisonnement du modèle est replié : une ligne d'état
réécrite en place pendant la réflexion, puis la réponse. `/think` affiche le
détail, `/exit` quitte.

---

## 7. Publication

`.github/workflows/release.yml`, déclenché par un tag `v*` :

| Artefact | Windows | Linux |
| --- | --- | --- |
| Serveur headless | `.msi` + `.zip` portable | `.deb` + `.tar.gz` |
| Application desktop | `.msi` + `.zip` portable | `.deb` / AppImage |
| Client Android | `.apk` (dès que `apps/mobile` existe) | — |

La release est créée en **brouillon**, jamais publiée automatiquement.

Le portable n'est pas un supplément : un poste verrouillé en entreprise ne peut
souvent pas exécuter d'installeur.

---

## 8. Activer le service depuis l'application

**Paramètres → Serveur & fonctions**, une case à cocher.

L'application ne sert **pas** le HTTP elle-même : elle supervise `locaryn-daemon`
avec `LOCARYN_DAEMON_BIND=0.0.0.0`. Le processus est un enfant privé du desktop,
redirigé vers le journal du service et créé avec `CREATE_NO_WINDOW` sous Windows :
aucune fenêtre CMD ne doit apparaître. Tout ce que le service garantit s'applique
donc sans duplication — authentification obligatoire, TLS, refus de démarrer
sans compte.

Le daemon lancé depuis cette carte appartient au desktop : l'arrêt explicite
via le menu du tray le termine avant de quitter Locaryn. La fermeture de la
fenêtre principale avec la croix Windows ne quitte pas l'application : elle la
masque dans le tray. Le tray propose **Ouvrir Locaryn** et **Quitter Locaryn** ;
seule cette seconde action réalise la sortie réelle et arrête le daemon.

Une seconde implémentation HTTP à l'intérieur de Tauri aurait signifié deux
endroits à garder corrects, et le plus critique des deux aurait été celui que
personne ne teste.

Ce que l'écran affiche :

- **Tant qu'aucun compte n'existe**, la case est désactivée et le motif est
  donné avec la commande exacte : `locaryn users add nom --admin`.
- **Une fois actif** : l'adresse à communiquer, le nombre de comptes,
  l'empreinte du certificat, et la commande `locaryn provision` déjà remplie avec
  l'adresse réelle de la machine.

L'état est revérifié toutes les cinq secondes : si le service s'arrête de
lui-même, l'interrupteur ne prétend pas le contraire.

---

## 9. mTLS — prouver que le client est légitime

**Optionnel, jamais activé automatiquement** : `require_client_cert`
(ou `LOCARYN_REQUIRE_CLIENT_CERT=1`). L'activer coupe tous les clients existants
jusqu'à ce que chacun ait reçu un certificat — cela doit être une décision, pas
la surprise d'une mise à jour.

Un mot de passe se devine, se hameçonne, se réutilise. Un certificat client ne
se tape pas dans une fausse page, et son absence fait échouer la poignée de main
**avant** tout échange applicatif : un scanner qui trouve le port ouvert
rencontre une connexion qui se ferme, pas un formulaire à attaquer.

### Émettre un certificat

```bash
locaryn users cert marie --days 365
```

```
Certificat + clé : ...\tls\clients\marie.pem
Autorité         : ...\tls\ca-cert.pem
```

Sur un serveur sans interface, ces chemins sont l'essentiel — rien n'ouvrira de
fenêtre proposant l'installation. Dans l'application, il est proposé à
l'installation sur l'écran de connexion — voir §11.

Les deux fichiers vont au poste de l'utilisateur : le premier prouve son
identité au serveur, le second lui permet de vérifier qu'il parle au bon
serveur. **Le premier est un secret.**

### Une seule autorité, dans les deux sens

Quand mTLS est actif, le certificat **du serveur** est lui aussi émis par
l'autorité locale, et non auto-signé.

Sans cela, un client qui fait confiance à l'autorité — ce qu'il doit faire pour
être vérifié par elle — rejette malgré tout le serveur, dont le certificat ne
chaîne vers rien de connu. C'était le cas au premier essai : le client était
accepté, le serveur refusé. Une autorité, les deux directions.

L'autorité est créée une fois et **jamais régénérée** : la refaire invaliderait
tous les certificats déjà distribués.

Vérifié de bout en bout : avec certificat `HTTP 200`, sans certificat la
connexion est refusée au niveau TLS.

---

## 10. Ouvrir un port sur la box

Pour la personne dont la machine calcule et qui est rarement à côté. La plupart
des box (Freebox, Livebox…) acceptent l'UPnP, donc personne n'a à ouvrir une
interface d'administration pour recopier des numéros de port.

```
open_router_port = true      (ou LOCARYN_OPEN_ROUTER_PORT=1)
```

### Refusé sans mTLS

```
Ouverture refusée : exposer ce serveur à Internet sans certificat client
le laisserait protégé par un simple mot de passe. Activez d'abord l'exigence
de certificat (require_client_cert), puis réessayez.
```

Ce n'est pas une précaution de principe. Publier un service sur Internet
derrière un seul mot de passe l'expose à tout le bruit de fond du réseau, et
l'automatiser sans condition serait pire : l'utilisateur ne saurait même pas
que c'est arrivé.

### Bail, pas trou permanent

La redirection est demandée avec un bail d'une heure, renouvelé tant que le
serveur tourne, et retirée à l'arrêt. Une box redémarrée l'oublie — c'est
voulu : une redirection permanente oubliée est précisément ce qui donne à
l'UPnP sa mauvaise réputation.

Si aucune box compatible ne répond, le serveur **continue** sur le réseau local
et le dit :

```
Aucune box compatible trouvée sur le réseau (No response within timeout).
Certaines box ont l'UPnP désactivé par défaut — activez-le, ou ajoutez la
redirection à la main.
```

---

## 11. L'écran de connexion

Sur un poste où un administrateur a déposé le fichier de déploiement,
l'application ne s'ouvre pas : elle demande d'abord un identifiant et un mot de
passe. Rien d'autre n'est à saisir — l'adresse, le nom de l'organisation et
l'empreinte à attendre viennent du fichier.

Sur une installation personnelle, aucun fichier n'existe et cet écran n'apparaît
jamais.

### Le certificat s'installe là

Le certificat client est proposé sur ce même écran, et reste gérable ensuite
dans *Paramètres → Partage réseau*. « Installer » signifie **l'enregistrer
auprès de Locaryn**, pas dans le magasin de certificats de Windows : le magasin
système le rendrait utilisable par n'importe quel programme de la machine, et
l'y importer demande des droits qu'un salarié n'a généralement pas — soit
exactement la friction que ce dispositif existe pour supprimer.

Le fichier est recopié dans le dossier de l'application, pour continuer à
fonctionner après un vidage du dossier Téléchargements.

Deux refus explicites, parce que les fichiers se ressemblent :

```
Ce fichier contient un certificat mais pas sa clé privée : il ne peut pas
servir à vous identifier. Demandez le fichier complet à votre administrateur.
```

```
Ce serveur exige un certificat client. Installez celui que votre
administrateur vous a transmis, puis réessayez.
```

Le nom affiché à côté du certificat est lu **dans le fichier** — c'est ce qui
permet de voir qu'on a installé le sien et pas celui d'un collègue.

### Ce que le client vérifie en retour

L'empreinte du fichier de déploiement sert enfin à quelque chose : le certificat
présenté par le serveur est comparé à celle-ci, et la connexion est refusée en
cas d'écart.

```
Le certificat présenté ne correspond pas à l'empreinte fournie par votre
administrateur. Ne saisissez pas votre mot de passe : signalez-le d'abord.
```

Le message dit de ne pas taper le mot de passe, parce que c'est précisément ce
qu'un intercepteur cherche à obtenir.

Quand le certificat de l'autorité est installé lui aussi, la vérification passe
par lui : le serveur peut alors renouveler son certificat sans que personne
n'ait à redistribuer une empreinte.

Sans empreinte ni autorité — le cas de quelqu'un qui parle au daemon de sa
propre machine — il n'y a rien à comparer et la connexion est acceptée.

### Ce qui est stocké

Seul le jeton renvoyé par le serveur, jamais le mot de passe. Un jeton se révoque
côté serveur sans obliger qui que ce soit à changer de mot de passe.

*Vérifié contre un serveur réel exigeant un certificat : avec certificat
`200 OK`, sans certificat la connexion est refusée pendant la poignée de main,
avec une empreinte fausse elle est refusée aussi, et un mauvais mot de passe
donne `401` — le certificat prouve la machine, jamais la personne.*

---

## 12. Accès distant : deux cas à ne pas confondre

**Entreprise** — réseau local, ou VPN déjà en place chez le client. Locaryn doit
seulement servir correctement sur le LAN. *Le VPN n'est pas à implémenter :
c'est l'infrastructure du client.*

**Utilisateur nomade** — sa machine calcule et reste chez lui ; il est souvent
ailleurs et n'a pas accès à la box pour ouvrir un port. D'où un tunnel
**sortant** : le PC appelle un relais, jamais l'inverse, ce qui traverse
n'importe quel réseau sans configuration.

Le tunnel est une commodité pour ce second cas, pas la brique entreprise.

### Le mode Remote

Optionnel : le plugin Remote s'installe ou non. Il apporte le tunnel sortant et
l'appairage du téléphone ; rien d'autre ne change quand il est absent.


```
travel = "cloudflare"      (ou LOCARYN_TRAVEL=cloudflare)
```

Trois relais, parce qu'aucun ne convient à tout le monde :

| Relais | Compte | Remarque |
|---|---|---|
| `cloudflare` | aucun | `cloudflared`, adresse en `*.trycloudflare.com` |
| `ngrok` | oui | déjà installé chez beaucoup de développeurs |
| `devtunnel` | oui (Microsoft) | le plus souvent autorisé en entreprise |

Aucun n'est embarqué dans Locaryn. Livrer le binaire d'un tiers, c'est livrer ses
mises à jour et ses failles ; l'outil est donc **détecté**, et s'il manque
l'interface dit quoi installer, avant que l'utilisateur ne choisisse — découvrir
qu'il faut un compte au moment de partir est le pire moment.

#### Ce qu'il refuse

Ouvrir un tunnel publie ce serveur sur Internet. C'est refusé tant que
l'authentification n'est pas réellement en vigueur :

```
Le mode Remote exposerait ce serveur à Internet alors qu'il n'exige aucune
authentification. Écoutez sur une adresse réseau (0.0.0.0) plutôt qu'en local,
ce qui rend l'authentification obligatoire, puis réessayez.
```

Il n'exige pas de certificat client, là où l'ouverture de port UPnP l'exige
(§10), et la différence est volontaire. Un port redirigé vit sur une adresse
stable que les scanners balaient en continu ; une adresse de relais est
aléatoire, non listée, et disparaît à la fermeture. Les deux sont une
exposition ; une seule se *trouve* sans chercher.

#### Le QR code, et pourquoi il est signé

Scanner un code change le serveur auquel l'application parle. C'est exactement
ce qu'un attaquant voudrait faire : imprimer un code, le faire scanner,
recevoir un mot de passe sur son propre serveur.

Le lien est donc **signé par l'autorité locale du déploiement** — la même qui
émet les certificats mTLS — et un téléphone ne l'accepte que pour un serveur
qu'il connaît déjà :

```
locaryn://travel?v=1&m=travel&u=<adresse>&e=<expiration>&k=<serveur>&s=<signature>
```

La signature couvre l'adresse elle-même : réécrire la destination dans un lien
authentique est l'attaque la moins chère qui soit, et elle invalide le lien.
Un code émis par quelqu'un d'autre donne :

```
Ce code n'a pas été émis par votre serveur. Ne l'utilisez pas.
```

Le message dit de ne pas saisir le mot de passe, parce que c'est précisément ce
que l'intercepteur cherche à obtenir. Un code expiré, lui, dit simplement d'en
afficher un nouveau — les deux situations ne demandent pas la même réaction.

Validité : dix minutes. Assez pour traverser une pièce, pas assez pour qu'une
photo de l'écran reste une clé.

#### Ce que l'utilisateur voit

Un interrupteur et un carré à photographier. **Aucune adresse, aucun port,
aucun nom de relais.** Le retour au réseau local se fait de la même façon : un
second code, signé pareil, que l'on scanne une fois rentré.

Sur un serveur sans interface, le code s'affiche dans le terminal :

```bash
locaryn travel on --via cloudflare
locaryn travel qr      # le code expire ; celui-ci en affiche un nouveau
locaryn travel home    # le code de retour
locaryn travel off
```

*Vérifié : tunnel ngrok réellement ouvert, adresse extraite, lien signé puis
vérifié, QR relu par un décodeur externe — la chaîne complète, du relais à
l'appareil photo.*

### Le téléphone

`apps/mobile` — Tauri v2, Android uniquement. Un client mince : les modèles,
les comptes et le chiffrement restent sur la machine d'en face.

Il **partage le Rust qui compte** : `locaryn-travel` vérifie un code scanné ici
exactement comme il le signe sur le serveur, donc les deux ne peuvent pas
diverger. Et il partage les jetons graphiques (`packages-ui/tokens/tokens.css`),
donc l'écran du téléphone parle la même langue que celui du bureau : même
échelle chaude, même vert unique, mêmes bordures d'un pixel.

#### Le chemin d'un code

1. Le téléphone enregistre un serveur une fois, depuis le fichier de
   déploiement. Ce fichier contient désormais **l'autorité** du déploiement —
   publique par nature — sans laquelle rien ne serait vérifiable.
2. L'appareil photo lit un code et propose d'ouvrir Locaryn.
3. Le lien est vérifié en Rust, puis l'adresse change.
4. L'écran dit « Vous êtes connecté », avec une coche qui se dessine et une
   brève dispersion dans les teintes de l'accent.

À aucun moment une adresse IP, un port ou un nom de relais n'apparaît.

#### Deux chemins, parce qu'un seul ne suffit pas

Android transmet un lien `locaryn://` quand l'appareil photo ou le navigateur
propose de l'ouvrir, et beaucoup le font. Mais beaucoup d'applications photo ne
présentent que les liens `http(s)` et restent muettes sur un schéma propre à
une application. Un vrai App Link `https` exigerait un domaine dont on contrôle
le fichier `assetlinks.json` — ce que Locaryn, installé chez le client, n'a pas.

Dire « scannez avec l'appareil photo » et qu'il ne se passe rien serait pire
qu'une pression de plus. Le **scanner intégré** est donc le chemin garanti, et
le lien direct la commodité quand elle marche.

#### Ce que le téléphone refuse

Les mêmes choses que partout ailleurs, avec les mêmes messages :

| Situation | Réponse |
|---|---|
| Code d'un autre serveur | « Ce code ne correspond à aucun serveur enregistré sur cet appareil. » |
| Adresse réécrite dans un lien authentique | « Ce code n'a pas été émis par votre serveur. Ne l'utilisez pas. » |
| Code périmé | « Ce code a expiré. Affichez-en un nouveau sur l'ordinateur. » |
| Étiquette de colis, code wifi | « Ce code ne vient pas de Locaryn. » |

Réinstaller le fichier de déploiement depuis un hôtel ne ramène pas le
téléphone sur une adresse locale qu'il ne peut pas joindre : l'état « en
voyage » survit à un ré-enregistrement.

#### Construire l'APK

```bash
cd apps/mobile
cargo tauri android build --apk --target aarch64
```

Il faut le SDK Android, un NDK, et `ANDROID_HOME` / `NDK_HOME`.

| Variante | Taille | Signature |
|---|---|---|
| `--apk` | ~9 Mo | **aucune** — ne s'installe pas tel quel |
| `--apk --debug` | ~161 Mo | clé de débogage Android ; s'installe tout de suite |

La version de diffusion sort **non signée** délibérément : la clé détermine
l'identité de l'application pour toutes ses mises à jour à venir. Une clé
inventée par l'outil de construction et laissée dans le dépôt serait la
mauvaise réponse à une question qui appartient au client.

Pour signer :

```bash
keytool -genkey -v -keystore locaryn.jks -keyalg RSA -keysize 4096 -validity 10000 -alias locaryn
apksigner sign --ks locaryn.jks app-universal-release-unsigned.apk
```

*Vérifié : l'APK construit déclare bien `dev.locaryn.mobile`, minSdk 24, la
permission caméra, et l'intention `locaryn://travel` — le lien d'appairage est
donc réellement enregistré dans l'artefact, pas seulement dans la source.*

---

## 13. Répartition de charge (à venir)

Le serveur fait tourner les gros modèles ; les postes clients suffisamment
puissants peuvent prendre les petites tâches en parallèle — une image, une
synthèse vocale, un texte court — pendant que le serveur traite le reste.

**L'invariant à ne jamais casser :** une tâche répartie ne s'exécute que sur les
machines de **l'utilisateur qui l'a demandée**. Jamais sur les postes des
collègues connectés au même serveur. Sinon on ponctionne les performances de
gens qui n'ont rien demandé, et l'outil se fait désinstaller.

Conséquence technique : la file de tâches distribuée doit être partitionnée par
**identité de compte**, pas seulement par machine disponible. C'est pourquoi
elle vient après l'authentification.

Corollaire recherché : un employé connecté avec le même compte sur deux postes
peut autoriser la répartition entre le serveur et ses deux machines — une image
sur le poste A, du code lourd sur le serveur, une autre image sur le poste B.

La répartition est **optionnelle**, activée côté client.

---

## 14. Ce qui reste

- Tunnel sortant, en complément de l'UPnP quand la box n'en veut pas — voir §12.
- Client mobile.
- Répartition de charge — voir §13.

---
