# État des dépôts Locaryn — 26/10/2026

Relevé fait le 26/10/2026 vers 22 h sur les 30 dépôts de l'organisation
(`.github` et `plugin-image-editor`, archivé, n'ont ni ticket ni PR).
Dernière release : **v0.4.50**, construite sur toutes les plateformes.

## En bref

| | Total | Ouverts | Fermés / fusionnés |
|---|---|---|---|
| Tickets | 91 | **22** (tous dans Locaryn) | 69 |
| Pull requests | 25 | **21** | 3 fusionnées, 1 fermée |
| Branches hors `main` | 25 | — | 1 entièrement fusionnée |

- Les 22 tickets ouverts : **16 ont déjà des correctifs partiels** (commits « Refs #N »),
  **6 n'ont jamais été touchés**.
- 18 des 21 PR ouvertes sont la même PR « extension → morph » ouverte il y a 18 jours
  dans 18 dépôts. Aucune n'a été fusionnée.
- Sécurité : 5 failles corrigées et publiées en v0.4.50, 6 points restent ouverts.
- Code dupliqué : 0,48 % (5 blocs). Code mort : 56 dépendances Rust et ~90 déclarations
  TypeScript retirées ; il en reste peu.

---

## 1. Tickets

### Fermés pendant cette session (7)

| # | Sujet |
|---|---|
| 81 | morph-dictaphone : le bouton micro ne dictait rien |
| 82 | « Actualiser » la liste des morphs prenait 15 à 30 s (désormais 1 à 2 s) |
| 83 | Désinstaller, désactiver ou mettre à jour un morph échouait (processus résiduels) |
| 84 | OmniRoute ne démarrait pas avec Locaryn ; installation npm coupée |
| 85 | OmniRoute listait des modèles inutilisables |
| 87 | Noms de morphs codés en dur dans le socle |
| 88 | Mises à jour automatiques de l'application et des morphs |

Les 62 autres tickets fermés l'ont tous été comme « terminé » (aucun « ne sera pas fait »).
Les dépôts de morphs n'ont que des tickets fermés (10 au total).

### Ouverts avec des correctifs partiels (16)

| # | Sujet | Ce qui manque | Âge |
|---|---|---|---|
| 86 | Sous-agents en parallèle sur modèles distants, modèle forcé | `/equipe` et Stop faits ; restent plusieurs modèles locaux en parallèle, `/equipe` sur mobile/service, pool du cluster | 0 j |
| 73 | morph-claude-code : se connecter depuis Locaryn | Connexion faite, fenêtre CMD retirée — **à fermer après ton essai** | 11 j |
| 71 | CLI : `locaryn` échoue sans service | Le CLI démarre son service ; l'agent de code reste à vérifier | 11 j |
| 70 | `/goal` fait un plan puis s'arrête | Corrigé le 15/10, jamais revérifié | 11 j |
| 63 | Plan : « Non abouti, essai 4/3 » | Corrigé le 15/10, jamais revérifié | 11 j |
| 69 | Prudence VRAM : refus en automatique, accepté en manuel | Refus aligné sur le menu ; à revérifier | 11 j |
| 61 | llama-server à ~75 % CPU sur RX 570 | Moteur allégé ; le GPU reste peu utilisé | 11 j |
| 66 | Navigateur intégré très lent | Actions ~0,6 s ; pas de mesure de bout en bout | 11 j |
| 67 | morph-desktop : Laya vraiment utilisé ? | Laya installé par l'app ; pas de mesure | 11 j |
| 74 | Snapdragon : NPU Hexagon | Détection + activation faites ; pas testé sur une vraie machine | 11 j |
| 75 | macOS Apple Silicon visible et ouvrable | Installeurs faits ; app non signée Apple | 11 j |
| 78 | morph-npu via Foundry Local | NPU sorti du socle vers un morph ; Foundry à brancher | 10 j |
| 54 | Panneau Tâches et workflows | Sous-agents + onglet Tâches faits ; pool distant manquant | 15 j |
| 35 | Mobile au niveau du bureau | Fait le 10/10 ; **jamais testé sur un vrai téléphone** | 16 j |
| 30 | Qwen3.5-9B en 8K ne lance pas les outils | Fenêtre mesurée en jetons réels ; à revérifier | 17 j |
| 21 | Espace de travail (terminal, aperçu, navigateur) de bout en bout | Question ouverte, pas de vérification complète | 17 j |

### Ouverts sans aucun travail (6)

| # | Sujet | Âge |
|---|---|---|
| 22 | Banc d'essai des morphs avec de petits modèles locaux | 17 j |
| 23 | Installation des morphs impossible chez l'entourage (à confirmer après v0.3.94) | 17 j |
| 25 | Installer Laya en un clic depuis morph-browser / morph-desktop | 17 j |
| 68 | Le modèle parle de « Laya » à l'utilisateur | 11 j |
| 76 | Installer Locaryn en une ligne de terminal | 10 j |
| 80 | CLI plein écran façon Claude Code, suggestions sur / et @ | 10 j |

Le #23 est sans doute réglé par les correctifs d'installation des 15/10 et 26/10
(le démon prend maintenant l'archive de release, flux Atom sans quota) : un essai
sur un PC tiers suffirait à le fermer.

---

## 2. Pull requests

### Ouvertes (21)

| Dépôt | # | Branche | Âge | État |
|---|---|---|---|---|
| 18 dépôts¹ | — | `docs/morph-terminology` | 18 j | « extension → morph » dans la doc ; jamais fusionnée |
| Locaryn | 59 | `l/gallant-fermi-lbhvnh` | 14 j | v0.4.6 + `/plan` ; 133 commits de retard, **obsolète** (refait dans main) |
| morph-remote | 1 | `feat/pairing-certificates` | 40 j | 5 commits utiles, fusionnable sans conflit |
| morph-image | 1 | `codex/studio-marketplace-integration` | 67 j | De Rea-PC08 ; **en conflit**, catalogue d'images déjà fait autrement |

¹ locaryn-cores, Locaryn (#14), morph-3d-gen, morph-cluster, morph-dictaphone,
morph-figures, morph-freetoken, morph-image, morph-model-training, morph-music-gen,
morph-prismml, morph-rag-qa, morph-remote, morph-text-analysis, morph-translation,
morph-video-gen, morph-vision-ocr, morph-voice-tts. Dans les morphs elles sont
fusionnables (0 à 5 commits de retard) ; dans Locaryn, 229 commits de retard et
`docs/extensions-interop.md` / `docs/writing-an-extension.md` parlent encore d'« extension ».

### Fusionnées ou fermées (4)

Locaryn #60 (release v0.4.9) et #3, #2 fusionnées ; #1 fermée.

### Recommandation

- Fusionner les 17 PR de terminologie des morphs et de locaryn-cores (sans risque), refaire celle de
  Locaryn à neuf sur main.
- Fusionner morph-remote #1 après relecture.
- Fermer Locaryn #59 et morph-image #1 avec un mot d'explication.

---

## 3. Branches

| Dépôt | Branche | Avance / retard | Dernier commit | Verdict |
|---|---|---|---|---|
| Locaryn | `feat/son-reponse` | +0 / −21 | 4 j | **Fusionnée** dans main le 26/10 → à supprimer |
| Locaryn | `fix/catalog-versions-beta-dot` | +1 / −286 | 22 j | Seul un commit de fusion d'avance → à supprimer |
| Locaryn | `rename/morph-remote` | +1 / −283 | 13 j | Seul un commit « v0.3.87 » d'avance (PR #2 fusionnée) → à supprimer |
| Locaryn | `freebuff/5131d3ff…` | +6 / −128 | 13 j | Ancienne v0.4.11, refaite dans main → expirée |
| Locaryn | `freebuff/f4fbe1bd…` | +1 / −110 | 11 j | Une retouche du README jamais reprise → à reprendre ou supprimer |
| Locaryn | `l/gallant-fermi-lbhvnh` | +2 / −133 | 14 j | PR #59, contenu refait dans main → expirée |
| Locaryn | `docs/morph-terminology` | +1 / −229 | 18 j | PR #14, très en retard |
| morph-remote | `feat/pairing-certificates` | +5 / −0 | 40 j | PR #1, à jour avec main, prête |
| 17 dépôts | `docs/morph-terminology` | +1 / −0 à −5 | 18 j | PR ouvertes, à jour ou presque |

Aucune branche active en cours de travail : tout ce qui vit est sur `main`.

---

## 4. Sécurité

Outils : cargo-deny (avis RustSec), `pnpm audit`, semgrep (règles Rust/TS/secrets),
relecture manuelle des routes publiques, recherche de secrets dans le dépôt et l'historique.

### Corrigé et publié en v0.4.50

| Gravité | Faille | Correctif |
|---|---|---|
| Haute | `/v1/travel` et `/v1/travel/home` publics : en mode serveur, n'importe qui sur le réseau pouvait ouvrir/couper le tunnel et récupérer le lien d'appairage | Réservé à la machine elle-même (loopback) + test |
| Haute | `/v1/auth/login` sans limite d'essais | Frein : 5 échecs / 15 min, puis blocage 60 s qui double jusqu'à 1 h, par adresse **et** par compte (429 + Retry-After) |
| Haute | `remote-server` (squelette) délivrait un jeton à tout le monde | Répond 501 |
| Moyenne | Archives de morphs : chemins absolus et bits SUID acceptés à l'extraction | Seuls les chemins relatifs simples passent, droits masqués à 0o777 |
| Moyenne | Dépendances vulnérables (rustls, rustls-webpki, tokio, crypto-bigint via russh, zip) | Mises à jour ; zip 9 ; russh figé en 0.63.1 |

Aucun secret trouvé dans le dépôt ni dans l'historique git.

### Reste ouvert

| Gravité | Point | Pourquoi c'est un risque | Piste |
|---|---|---|---|
| Haute | Les panneaux de morph s'exécutent par `new Function` dans la fenêtre principale | Un morph tiers malveillant a tous les pouvoirs de l'app (commandes Tauri, fichiers) | Iframe isolée + pont par `postMessage` avec liste de commandes autorisées |
| Moyenne | `assetProtocol` autorisé sur `**` | La page peut lire n'importe quel fichier du disque via `asset://` | Restreindre aux dossiers de données et de modèles |
| Moyenne | CSP `script-src 'unsafe-eval' 'unsafe-inline'` | Annule la protection contre l'injection de script | Lié au premier point : à retirer une fois les panneaux isolés |
| Moyenne | Mobile : `register_address` enregistre un serveur sans épingler son autorité | Un faux serveur sur le réseau peut se faire enregistrer | Exiger le QR signé, comme l'appairage |
| Basse | `reqwest danger_accept_invalid_certs` (mobile `lib.rs` 218/318/363) | Certificat non vérifié sur ces appels | Vérifier qu'ils sont tous suivis d'un épinglage (c'est le cas dans `serveurs.rs`) |
| Basse | vite 5 : 9 avis restants | Serveur de développement seulement, pas l'app livrée | Passer à vite 6 |
| Info | 3 avis RustSec ignorés (zip 4.6.1 tiré par l'updater de Tauri) | Hors de notre code, justifié dans `deny.toml` | Attendre la mise à jour du plugin |
| Info | La connexion admin par QR (`login_qr`) ne marche jamais à distance | Les routes publiques sautent le contrôle du jeton : fonction cassée, pas une faille | À corriger ou retirer |

Semgrep : 118 résultats, 0 erreur. Hors les points ci-dessus, ce sont des
informations sans danger dans ce contexte (72 dossiers temporaires, 28 blocs
`unsafe` déjà commentés, 5 `current_exe`, 4 lectures d'arguments ; 2
`dangerouslySetInnerHTML` dont le contenu est échappé).

À part : ta clé NVIDIA a été écrite en clair dans la conversation — **régénère-la**.

---

## 5. Code dupliqué

jscpd sur tout le TypeScript/Rust : **0,48 %** (5 blocs, 535 lignes).

| Blocs | Lignes | Piste |
|---|---|---|
| `FiguresScreen.tsx` mobile ↔ web | ~460 | Le déplacer dans `packages/ui` |
| `ExtensionSlot` bureau ↔ mobile | 45 | Idem |
| `Studio` mobile ↔ web | 32 | Idem |
| 2 petits blocs | ~0 | Négligeables |

Côté Rust, aucune duplication notable. Pendant la session, la libération des
processus d'un dossier (bureau + passerelles) a été rassemblée dans
`locaryn_mcp::arbre`.

---

## 6. Code mort

### Retiré (commits cc376cb et ef11f5e)

- 56 dépendances Rust inutilisées dans 21 `Cargo.toml` (cargo-machete).
- 4 fichiers TypeScript jamais importés (`useLayout.ts`, `modelList.ts`,
  `PreviewPanel.tsx`, `TrainingView.tsx`).
- 53 exports jamais importés ailleurs, rendus privés ; 34 déclarations mortes
  supprimées ; imports inutilisés retirés (knip + `tsc --noUnusedLocals`).

### Restant

| Élément | Où | Remarque |
|---|---|---|
| `slashInline` | bureau | Écrit, jamais lu : sans doute une fonction perdue, à décider |
| `info` | `SettingsView` | Valeur inutilisée |
| `notifPermission` | réglages mobiles | Valeur inutilisée |
| `chat`, `preview`, `terminal` | `packages/ui` | 130 lignes jamais importées, mais listées comme squelettes dans la feuille de route |
| 14 `#[allow(dead_code)]` | Rust | À revoir un par un |

---

## Ce qui reste à faire, par ordre

1. Isoler les panneaux de morph (sécurité haute), puis resserrer la CSP et `assetProtocol`.
2. Fusionner ou fermer les 21 PR, supprimer les 5 ou 6 branches expirées de Locaryn.
3. Revérifier et fermer les tickets corrigés mais jamais retestés (#63, #69, #70, #73).
4. Tester l'app mobile sur un vrai téléphone (#35) et la mise à jour silencieuse de bout en bout.
5. Finir #86 (modèles locaux en parallèle, cluster).
