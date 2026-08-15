# 13 — Client web auto-hébergé + PWA

> Objectif : travailler depuis n'importe quel navigateur (téléphone, tablette,
> PC sans installation), sur un site web servi par le daemon lui-même, avec
> installation PWA — en particulier pour iPhone, sans abonnement Apple
> Developer.

## Pourquoi c'est naturel ici

Le daemon est déjà un serveur HTTP (axum) avec :

- une auth par jeton : `POST /v1/auth/login` → `{ token, user }`, en-tête
  `Authorization: Bearer <jeton>` exigé dès que le serveur est exposé
  (`require_token`, `auth.rs`) ;
- TLS rustls quand il écoute ailleurs que loopback, mTLS optionnel ;
- un relais « mode voyage » (travel) qui publie déjà le serveur sur une adresse
  HTTPS stable — c'est lui qui rend les liens de pairing accessibles de
  l'extérieur ;
- le mobile est déjà un client HTTP pur (chat via projets → sessions → SSE,
  studio via `/v1/media/*`) : le web réutilise le même contrat, éprouvé.

Servir le site depuis le daemon, **même origine** que `/v1/*`, supprime CORS,
mixed content et gestion de certificat côté navigateur.

## 1. `apps/web` — le client web

Vite + React, même langage visuel que le mobile : tokens partagés
(`packages-ui/tokens`), classes `lo-*`, police Sen embarquée. Le client web est
le mobile sans Tauri : les composants d'écran (Connexion, Chat, Studio,
Appairage) sont réutilisés tels quels, seul le « core » change.

**Core HTTP** (`apps/web/src/lib/core.ts`) — remplace `tauri invoke` et le
`demoCore` par un vrai client fetch :

- `sign_in(username, password)` → `POST /v1/auth/login` → jeton stocké dans
  `localStorage` (`locaryn.session`) — jamais le mot de passe ;
- `status()` → `GET /v1/auth/me` (valide le jeton au chargement) ;
- `send(text)` → mêmes appels que le mobile : retrouver/créer le projet
  free-chat, créer une session, `POST /v1/sessions/{id}/messages` — mais en
  **`EventSource` natif** (le navigateur parse le SSE lui-même, on agrège les
  events `token`) ;
- `listMediaModels` / `generateImage` / `generateAudio` → `/v1/media/*` ;
- `signOut()` → efface le jeton local (+ révocation côté daemon si dispo).

Écrans Phase 1 (parité mobile) : Connexion, Chat, Studio Image/Voix.
Écrans Phase 2 (parité desktop) : projets/sessions, fournisseurs + supervisor,
outils MCP, historique.

## 2. Daemon — servir le site

- `tower-http` feature `fs` : `ServeDir` sur `{data_dir}/web` + **fallback SPA**
  (toute route inconnue renvoie `index.html`) ;
- MIME corrects : `.webmanifest` → `application/manifest+json`, `.js`, `.css`,
  `.woff2`, icônes ;
- route `/` = le site, `/health` et `/v1/*` inchangés ;
- build : `pnpm --dir apps/web build` puis copie de `dist` → `{data_dir}/web`
  (script npm dédié).

Décision recommandée : servir depuis `{data_dir}/web` (runtime) plutôt que
d'embarquer le bundle dans le binaire — le site se met à jour sans rebuild du
daemon.

## 3. PWA

- **`manifest.webmanifest`** : `name: Locaryn`, `display: standalone`,
  `background_color`/`theme_color` `#17191a`, icônes 192/512 ;
- **service worker** : pré-cache du app shell (bundle + fonts), stratégie
  cache-first pour les assets, jamais pour `/v1/*` (l'API reste réseau) ;
  enregistrement seulement en contexte sécurisé ;
- **iPhone** (pas de `beforeinstallprompt`) : détection
  (`iPhone|iPad` dans le user-agent, pas en `display-mode: standalone`) → popup
  maison « Installer Locaryn » avec les instructions
  *Partager → Ajouter à l'écran d'accueil*, bouton « Pas maintenant » ;
- **Android** : `beforeinstallprompt` → invite native ;
- **mémorisation** (le point demandé) : la réponse de l'utilisateur
  (installé / refusé) est enregistrée dans `localStorage` (`locaryn.pwa` avec
  date) — plus jamais reproposé à une visite suivante. On re-vérifie au retour
  au premier plan : une fois en mode `standalone`, plus aucune popup.

### Contrainte HTTPS — à documenter pour l'utilisateur

Service worker et manifest n'existent qu'en **contexte sécurisé** (HTTPS ou
localhost). Conséquences :

- en LAN sur `http://192.168.x.x:7474`, le site fonctionne comme site web
  normal, mais sans install PWA ;
- la PWA complète s'active dès que le site est servi en HTTPS — c'est déjà le
  cas via le **relais travel** existant (adresse HTTPS stable, la même que les
  liens de pairing) ; alternative : nom de domaine + Let's Encrypt + redirection
  de port, en réutilisant le TLS du daemon (voir §5 et doc 12 §10).

## 4. Sécurité

- le site web affiche toujours l'écran de connexion, même en loopback (décision
  produit : le web est un produit, pas un outil d'admin ; `/v1` reste ouvert en
  local pour les clients natifs comme aujourd'hui) ;
- exposé : tokens requis partout (déjà assuré par `require_token`) ;
- mots de passe jamais stockés ; jeton seul en `localStorage`, effacé au
  `sign_out` ;
- CSP stricte pour le bundle web.

## 5. HTTPS grand public — domaine + Let's Encrypt

> Option documentée pour sortir du relais travel : servir le site sur un
> domaine avec un certificat public. La PWA (service worker + manifest)
> exige HTTPS, et un certificat Let's Encrypt est la manière standard de
> l'obtenir sans autorité privée.

### Ce qui est déjà là : le TLS du daemon

Le daemon sait déjà servir HTTPS avec un certificat **fourni** :

- `daemon.tls_cert` / `daemon.tls_key` pointent vers deux fichiers PEM ;
  `tls::resolve` les vérifie (fichiers présents, sinon échec bruyant — jamais
  de repli en clair) et `axum_server::bind_rustls` les sert (main.rs ~294-378) ;
- sans ces deux clés, un certificat auto-signé est généré et réutilisé ;
- la même config sert déjà le relais travel et le mTLS optionnel.

Un certificat Let's Encrypt est exactement un « certificat fourni » : il n'y a
**rien à coder**, juste à obtenir le certificat et à pointer la config dessus.

### Procédure documentée

1. **Domaine** — enregistrer un nom de domaine et le faire pointer (A/AAAA)
   vers l'IP publique de la machine ; en IP dynamique, un DDNS (duckdns,
   no-ip, …) suffit.
2. **Redirection de port** — sur la box/routeur, rediriger `443` (TCP) vers la
   machine. Deux possibilités : le daemon écoute directement sur `443`
   (`port = 443`), ou sur un port >1024 avec une règle NAT externe
   `443 → <ip>:<port>`. Documenter selon le fournisseur d'accès (certains
   bloquent 80/443 entrants → port non standard + redirection).
3. **Obtenir le certificat** — deux modes certbot :
   - `certbot certonly --standalone -d <domaine>` : certbot écoute sur 80/443
     à la place du daemon (l'arrêter pendant l'émission, ~30 s) ;
   - `certbot certonly --webroot -w {data_dir}/web -d <domaine>` : le daemon
     sert déjà `{data_dir}/web`, donc `.well-known/acme-challenge/` est servi
     tel quel par `ServeDir` — **aucune coupure**.
4. **Config daemon** :
   ```toml
   [daemon]
   bind = "0.0.0.0"
   port = 443
   tls_cert = "/etc/letsencrypt/live/<domaine>/fullchain.pem"
   tls_key  = "/etc/letsencrypt/live/<domaine>/privkey.pem"
   require_client_cert = false   # requis : les navigateurs ne présentent pas
                                 # de certificat client, le web public l'exige off
   ```
5. **Renouvellement** — les certificats LE vivent ~90 jours. Le daemon lit les
   PEM au démarrage : brancher `--deploy-hook` (redémarrage du service daemon)
   sur le renouvellement. Option future : rechargement à chaud des certificats
   (watch des fichiers + reconfigure rustls) pour éviter le restart.

### Résultat et limites

- site servi en HTTPS public → service worker + manifest actifs → **PWA
  installable partout, iPhone compris**, sans relais travel et sans compte
  Apple Developer ;
- une fois public (`bind` ≠ loopback), `require_token` s'applique : le site web
  reste whitelisté (`is_public`), l'API `/v1/*` exige le jeton — déjà en place ;
- comparé au relais travel : domaine = contrôle total (latence, disponibilité,
  pas de dépendance au relais) mais demande domaine + redirection de port +
  renouvellement ; relais = zéro configuration, adresse HTTPS stable, dépend
  du relais. Les deux coexistent (le relais reste utile pour le pairing
  ponctuel).

## 6. Tests

1. build `apps/web` + parcours navigateur (connexion, chat SSE, studio image +
   audio) contre un daemon réel ;
2. PWA : Chrome DevTools (Application → Manifest / Service Workers), émulation
   iPhone et Android ;
3. test réel : daemon exposé via le relais travel → ouvrir l'URL HTTPS depuis
   un iPhone → popup d'installation → mode `standalone` → chat + studio
   fonctionnels ;
4. non-reproposition : refuser, recharger, vérifier l'absence de popup ;
   réinstaller, vérifier l'absence de popup.

## Phases

- **Phase 1 (MVP)** : `apps/web` (connexion, chat SSE, studio), daemon sert
  `/web`, manifest + SW, popup iOS + invite Android, mémorisation localStorage.
- **Phase 2** : parité desktop (projets/agents, supervisor, outils MCP),
  pairing web par code QR affiché (au lieu du scan caméra), historique.
- **Phase 3** : HTTPS public (domaine + Let's Encrypt, §5), web push, sessions
  synchronisées multi-appareils.
