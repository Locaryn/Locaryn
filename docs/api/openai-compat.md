# Locaryn API — Compatibilité OpenAI & Circuits d'accès

## 1. Endpoints compatibles OpenAI (outils tiers : VS Code, scripts, etc.)

Base URL : `https://<votre-ip>:7474` (ou l'URL tunnel fournie par l'app).

| Endpoint | Méthode | Description |
|---|---|---|
| `/v1/models` | GET | Liste des modèles disponibles (format OpenAI) |
| `/v1/chat/completions` | POST | Chat/complétions (format OpenAI, streaming SSE supporté) |
| `/v1/auth/tokens` | GET/POST | Liste / création de clés API développeur |
| `/v1/auth/tokens/:id/revoke` | POST | Révocation d'une clé API |
| `/v1/auth/tokens` (DELETE `?id=`) | DELETE | Suppression d'une clé API |
| `/v1/auth/tokens/devices` | GET | Liste des appareils connectés (Circuit B) |
| `/v1/auth/tokens/devices/:id/revoke` | POST | Déconnexion d'un appareil |
| `/v1/auth/pair/confirm` | POST | Validation du code à 6 chiffres (Circuit B, public) |
| `/v1/auth/login` | POST | Connexion classique utilisateur/mot de passe |

### Authentification

Tous les endpoints (sauf `/v1/auth/pair/confirm` et `/v1/auth/login`) acceptent
le header standard :

```
Authorization: Bearer <token>
```

Deux types de tokens sont acceptés, tous deux via Bearer :

- **Clé API développeur** (préfixe `locaryn_`) — Circuit A, créée manuellement.
- **Token de session appareil** — Circuit B, délivré après login ou appairage QR.

### Exemple avec un outil tiers (VS Code / Continue / curl)

```bash
curl https://192.168.1.10:7474/v1/chat/completions \
  -H "Authorization: Bearer locaryn_votre_cle" \
  -H "Content-Type: application/json" \
  -d '{"model":"llama3","messages":[{"role":"user","content":"Bonjour"}]}'
```

CORS est activé sur `/v1/models` et `/v1/chat/completions` (OPTIONS préflight
géré), ce qui permet l'usage depuis des web-apps tierces.

## 2. Circuit A — Clés API développeur

Création manuelle depuis l'interface web (Paramètres → Clés API) ou via l'API :

```bash
# Créer une clé (le plaintext n'est affiché qu'une fois)
curl -X POST https://<ip>:7474/v1/auth/tokens \
  -H "Authorization: Bearer <session>" \
  -H "Content-Type: application/json" \
  -d '{"label":"vs-code","expiresInDays":90}'

# Lister (hint = 6 premiers caractères, jamais le plaintext)
curl https://<ip>:7474/v1/auth/tokens -H "Authorization: Bearer <session>"

# Révoquer
curl -X POST https://<ip>:7474/v1/auth/tokens/<id>/revoke \
  -H "Authorization: Bearer <session>"
```

- Préfixe `locaryn_`, affiché **une seule fois** à la création.
- Durées : 7 / 30 / 90 jours ou jamais (défaut).
- Section UI dédiée « Clés API », séparée des appareils.

## 3. Circuit B — Appairage d'appareils

### Connexion classique
`POST /v1/auth/login` `{username, password, label}` → token de session appareil
(180 jours), dédié à l'appareil.

### Appairage QR + code à usage unique (double authentification)

1. L'hôte affiche un QR code (écran PairingCodes du desktop) contenant la
   config serveur signée + un **code à 6 chiffres**.
2. Le client scanne le QR → `POST /v1/auth/pair/confirm`
   `{pairing_code, device_label}`.
3. Le code est valide **2 minutes**, à **usage unique**, max **5 essais**
   (comparaison en temps constant, rate-limit).
4. Le serveur délivre un token de session appareil dédié (180 jours).

```
POST /v1/auth/pair/confirm
{"pairing_code":"482913","device_label":"iPhone de Teano"}

→ 200 {"token":"<session>","expires_at":"...","device_label":"..."}
→ 400 code malformé · 401 code incorrect/expiré · 409 pas d'admin
```

## 4. Séparation UI (Paramètres du compte)

- **Clés API** : liste uniquement les tokens `kind=api` (création, révocation,
  hint 6 caractères — le plaintext n'est jamais ré-affiché).
- **Appareils connectés** : liste les tokens `kind=session` (nom, label,
  métadonnées, date, révocation). Le token sous-jacent reste masqué.

## 5. Structure multi-standards

`services/daemon/src/routes/openai.rs` est organisé en couche dialecte :
le cœur de génération est commun, les adapteurs OpenAI (actif) puis
Anthropic/Ollama (structure en place) traduisent requêtes/réponses.
