# Collecteur de journaux

Reçoit les rapports anonymes de l'application de bureau (`apps/desktop/src-tauri/src/rapports.rs`).
Python, bibliothèque standard seulement, SQLite.

En ligne : `https://arthonetwork.fr/locaryn/api/logs`. Sur le serveur, dans `~/locaryn-logs`
(conteneur `locaryn-logs`, `127.0.0.1:4510`), derrière le Caddy existant
(`~/caddy-proxy/Caddyfile`, bloc `arthonetwork.fr`, `handle /locaryn/api/logs*`).

| Route | Rôle |
| --- | --- |
| `POST /locaryn/api/logs` | Un rapport JSON. Sans authentification. 3 Mo au plus (4 Mo coupés par Caddy). |
| `GET /locaryn/api/logs/health` | État du service. |
| `GET /locaryn/api/logs/admin` | Liste filtrable : gravité (bug critique), note (pouce levé ou baissé), genre, version, recherche. Authentification Basic, n'importe quel nom, mot de passe `ADMIN_PASSWORD`. |
| `GET /locaryn/api/logs/admin/<id>` | Un rapport complet. |
| `GET /locaryn/api/logs/admin/json` | La liste en JSON (mêmes filtres en paramètres). |

Garde-fous :

- 20 rapports par heure et par IP, 5 s entre deux. L'IP n'est gardée qu'en mémoire, hachée ; jamais écrite.
- Deuxième nettoyage côté serveur (e-mails, chemins personnels, clés, jetons, IP locales).
- Rétention : 30 jours, et 200 Mo au plus (les plus anciens partent d'abord).

Corps attendu : `genre` (`erreur`, `note`, `probleme`, `manuel`), `gravite` (`critique`, `erreur`,
`info`), `note` (`up`, `down`), `version`, `os`, `arch`, `modele`, `morphs` (liste), `titre`,
`detail`, `journaux`, `echange` (`note` et `probleme` seulement).

Mot de passe de consultation : `~/locaryn-logs/.env` sur le serveur (fichier en 600, hors dépôt).

Déployer une modification :

```bash
scp services/log-collector/app.py ha.arthonetwork.fr:locaryn-logs/
ssh ha.arthonetwork.fr 'docker restart locaryn-logs'
```
