<div align="center">

# 🌟 LOCARYN

**The Universal Open-Core AI Development, Agentic Platform & Model Studio**

*Fast, Private, Native, and Deeply Extensible — Powered by a Single Shared Rust Core.*

[![GitHub Release](https://img.shields.io/github/v/release/Locaryn/locaryn?color=10b981&label=release&logo=github)](https://github.com/Locaryn/Locaryn/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg?logo=rust)](https://www.rust-lang.org)
[![Tauri v2](https://img.shields.io/badge/tauri-v2-24c8db.svg?logo=tauri)](https://tauri.app)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux%20%7C%20Android%20%7C%20Web-purple.svg)](https://github.com/Locaryn/locaryn/releases)
[![Stars](https://img.shields.io/github/stars/Locaryn/locaryn?style=flat&color=yellow)](https://github.com/Locaryn/locaryn)

[✨ Points Forts](#-pourquoi-locaryn---points-forts-et-avantages-clés) •
[🏛 Architecture](#-architecture-unifiée) •
[🧩 Extensions & Noyau](#-écosystème-dextensions-morphs) •
[🧠 Natif ou Morph ?](#-studio-de-modèles--natif-et-morph-pour-lentraînement) •
[🚀 Démarrage Rapide](#-démarrage-rapide) •
[📥 Téléchargements](#-téléchargements--releases)

</div>

---

## 📖 Présentation

**Locaryn** est une plateforme complète d'intelligence artificielle locale et distribuée, conçue pour unifier le développement assisté par IA, la gestion et modification de modèles (GGUF, HuggingFace, registre Ollama, APIs distantes), l'orchestration multi-agents et la collaboration multi-appareils.

Bâtie autour d'un **noyau partagé 100% Rust**, Locaryn combine une application de bureau native ultra-rapide (Tauri v2 + React), une CLI ergonomique, un serveur sécurisé (daemon headless) et une application mobile compagne (Android/iOS).

---

## ⚡ Pourquoi Locaryn ? — Points Forts et Avantages Clés

Contrairement aux solutions cloisonnées ou dépendantes du cloud propriétaire, Locaryn apporte une flexibilité inédite :

### 1. 🔄 Rôle Dual Client ⇄ Serveur Universel
- **Chaque client peut être serveur, et chaque serveur peut être client** : Utilisez votre PC fixe équipé d'un GPU comme serveur local ou distant, et pilotez-le en toute transparence depuis votre PC portable, votre tablette ou votre smartphone.
- **Appairage Instantané et Sécurisé** : Rejoignez une machine en scannant un QR code ou via le **Mode Découverte** sur le réseau local Wi-Fi, avec chiffrement de bout en bout (mTLS avec certificats auto-générés). Ces deux modes sont **natifs** ; ouvrir la machine de l'extérieur du réseau — port exposé ou tunnel sortant (Cloudflare, ngrok, devtunnel) — est, lui, **apporté par un morph** (capacité `travel-tunnel`).
- **Mode Éphémère & Vie Privée** : Vos échanges et sessions peuvent être basculés en mode éphémère d'un clic pour garantir une confidentialité totale sans enregistrement résiduel.

### 2. 🧩 Extensibilité Totale du Noyau (Core Extensibility)
- Les morphs dans Locaryn ne sont pas de simples gadgets : ils ont la capacité de **modifier le comportement même du noyau (core runtime)**, d'enregistrer des serveurs MCP natifs, d'intercepter les événements du système (hooks) et d'injecter des **vues et menus complets dans l'interface graphique** (ex: *Figures Académiques*, *Studio de Création*, *Fine-tuning & Oblitération*).
- **Compatibilité Universelle Cross-Écosystème** : Locaryn exécute et adapte nativement les extensions et skills provenant de **Locaryn**, **Claude Code**, **Gemini CLI**, **OpenCode** et du **Model Context Protocol (MCP)**. Un simple lien GitHub (`github:owner/repo`) installe n'importe quel morph ou skill en un clic.

### 3. 🧠 Du Débutant au Chercheur / Power-User
- **Pour tous les utilisateurs** : Une interface moderne, épurée et réactive, moteur local **llama.cpp embarqué** (LM Studio et vLLM sont utilisés lorsqu'ils tournent déjà), catalogue de modèles HuggingFace / registre Ollama, et fournisseurs distants compatibles OpenAI (OpenRouter…) apportés par les morphs — les clés restent dans le trousseau du système.
- **Pour les créateurs et chercheurs** — chacune de ces fonctions est **apportée par un morph**, le socle ne les contient pas :
  - **Fine-Tuning & LoRA** : [`morph-model-training`](https://github.com/Locaryn/morph-model-training) (capacité `model-training`). llama.cpp ne fait que de l'inférence : l'entraînement reste une pile Python (Unsloth / PEFT), dont l'export GGUF se recharge ici.
  - **Oblitération RepE (Representation Engineering)** : même morph — analyse et ablation directionnelle des couches de refus, avec avertissement explicite de responsabilité.
  - **Figures & Visualisation** : stockage et écran natifs, contenu (`figures/*.md`) importé avec le morph [`morph-figures`](https://github.com/Locaryn/morph-figures) (capacité `figures`) : sans lui, l'entrée n'apparaît pas.
  - **Espaces de travail distants (SSH)** : les cibles sont déclarées par un morph (`ssh-remote-exec`, [`morph-ssh`](https://github.com/Locaryn/morph-ssh)) ; l'application ne connaît aucun hôte à l'avance.

### 4. 🚀 Performance & Sobriété
- **100% Rust** : Zéro surcharge Electron, empreinte mémoire minimale, démarrage instantané et zéro télémétrie non sollicitée.

---

## 📊 Matrice Comparative

| Fonctionnalité | Locaryn | LM Studio / Ollama | Claude Desktop | Cursor / OpenCode |
| :--- | :---: | :---: | :---: | :---: |
| **Noyau Natif Rust ultra-léger** | ✅ Oui | ❌ Non / Partiel | ❌ Non (Electron) | ❌ Non |
| **Client ⇄ Serveur Interchangeable** | ✅ Oui (Desktop/Mobile/CLI/Web) | ⚠️ Serveur local seul | ❌ Non | ❌ Non |
| **Appairage Mobile (QR / Découverte Wi-Fi)** | ✅ Oui | ❌ Non | ❌ Non | ❌ Non |
| **Morphs modifiant l'UI & le Noyau** | ✅ Oui (Deep Extensibility) | ❌ Non | ⚠️ MCP seul | ⚠️ Extensions limitées |
| **Compatibilité Claude / Gemini / MCP / OpenCode** | ✅ 100% universel | ❌ Non | ⚠️ MCP seul | ⚠️ Partiel |
| **Fine-Tuning & Oblitération RepE** | ✅ Oui (via morph `model-training`) | ❌ Non | ❌ Non | ❌ Non |
| **Génération Multimédia (Image, TTS, 3D, Figures)** | ✅ Oui (via morphs dédiés) | ❌ Non | ❌ Non | ❌ Non |

> Les lignes marquées « via morph » ne s'affichent qu'une fois l'extension correspondante installée et activée : le socle ne contient ni écran, ni outil, ni réglage propre à ces fonctions, et les retire avec le morph.

---

## 🏛 Architecture Unifiée

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           INTERFACES LOCARYN                            │
│   Desktop (Tauri v2 / React) │ Mobile App (Android/iOS) │ CLI │ Web UI  │
└────────────────────────────────────┬────────────────────────────────────┘
                                     │ (mTLS / HTTP / SSE / WebSocket)
┌────────────────────────────────────▼────────────────────────────────────┐
│                        NOYAU PARTAGÉ (RUST CORE)                        │
│ ┌──────────────────────┐ ┌─────────────────────┐ ┌───────────────────┐  │
│ │ locaryn-agent-runtime│ │ locaryn-extensions  │ │  locaryn-mcp      │  │
│ └──────────────────────┘ └─────────────────────┘ └───────────────────┘  │
│ ┌──────────────────────┐ ┌─────────────────────┐ ┌───────────────────┐  │
│ │ locaryn-skill-runtime│ │ locaryn-hook-runtime│ │ locaryn-travel    │  │
│ └──────────────────────┘ └─────────────────────┘ └───────────────────┘  │
│ ┌──────────────────────┐ ┌─────────────────────┐ ┌───────────────────┐  │
│ │ locaryn-core-bridge  │ │ locaryn-storage     │ │ locaryn-llmfit    │  │
│ └──────────────────────┘ └─────────────────────┘ └───────────────────┘  │
└────────────────────────────────────┬────────────────────────────────────┘
                                     │
┌────────────────────────────────────▼────────────────────────────────────┐
│                      MOTEURS & FOURNISSEURS IA                          │
│   llama.cpp │ LM Studio │ vLLM │ APIs compatibles OpenAI │ HuggingFace  │
└─────────────────────────────────────────────────────────────────────────┘
```

> `llama.cpp` est le seul moteur **embarqué** ; LM Studio et vLLM sont pilotés lorsqu'ils tournent déjà. Les sources de modèles (HuggingFace, registre Ollama) et les fournisseurs distants compatibles OpenAI viennent, eux, des morphs — de même que tout ce qui n'apparaît pas dans le schéma : images, voix, figures, SSH, tunnels… et **entraînement**. Le cœur compte une trentaine de crates (`command-runtime`, `rules-runtime`, `lsp-adapters`, `media`, `preview`, `auth`, `cloud-providers`…), liste complète dans [`Cargo.toml`](Cargo.toml).

---

## 🧩 Écosystème d'Extensions (Morphs)

L'application ne connaît aucune extension à l'avance : tout ce qu'un morph
apporte — studio, catégories et modèles du Marketplace, outils, réglages —
n'existe que s'il est installé et activé, et disparaît avec lui.

Les morphs officiels sont les dépôts
[`morph-*` de l'organisation](https://github.com/orgs/Locaryn/repositories?q=morph-) ;
le catalogue de l'application les relève tout seul, avec leurs versions.

### 🧭 Ce qui est natif, ce qui vient d'un morph

Le socle ne livre que ce qui marche sans rien installer ; tout le reste est déclaré par un manifeste `morph.json`.

| Ce que vous utilisez | Origine |
| :--- | :--- |
| Chat agentique, conversations, projets, outils fichiers & commandes, sous-agents | **Natif** |
| Catalogue de modèles, téléchargement, choix de quantification, résidence des poids | **Natif** |
| Moteur llama.cpp : adaptateurs LoRA à chaud, cache KV compressé, offload, décodage spéculatif | **Natif** |
| Appairage local (QR, mDNS, mTLS), conversations éphémères, preview d'artefacts | **Natif** |
| Entrées de menu, onglets du Studio, écran Figures, Batch API | **Morphs** (capacités `figures`, `batch-api` et contributions `nav_items` / `studio_tabs`) |
| Entraînement LoRA / QLoRA, distillation, oblitération RepE | **Morph** [`morph-model-training`](https://github.com/Locaryn/morph-model-training) (capacité `model-training`) |
| Images, voix, musique, vidéo, 3D, OCR, traduction, RAG | **Morphs** dédiés |
| Cibles SSH, tunnel d'accès distant, fournisseurs cloud et leurs clés | **Morphs** (`ssh-remote-exec`, `travel-tunnel`, section `cloud_provider`) |
| Catégories et modèles du Marketplace | **Morphs** actifs |

Retirez le morph : son écran, ses réglages, ses outils et ses capacités disparaissent avec lui, sur l'ordinateur comme sur le téléphone.

### 🛠 Écrire la vôtre

Une extension est un **produit distinct** : votre dépôt, votre rythme, votre nom.
Rien ne se soumet nulle part — vous publiez, l'utilisateur colle l'adresse.

Le guide complet est dans **[`docs/writing-an-extension.md`](docs/writing-an-extension.md)** :
manifeste minimal, composants (skills, commandes, agents, règles, hooks, MCP,
LSP), contributions d'interface avec une forme par plateforme, panneau qui hérite
du thème de l'application, serveur MCP compilé et publié par plateforme,
catalogue de modèles qui se rafraîchit tout seul, permissions, publication et
mise à jour.

L'exemple minimal vit dans [`examples/plugins/my-plugin`](examples/plugins/my-plugin) ;
pour un exemple complet, avec binaire, panneau et catalogue, lisez n'importe
quel [morph officiel](https://github.com/orgs/Locaryn/repositories?q=morph-).

---

## 🧠 Studio de Modèles : Natif, et Morph pour l'Entraînement

Le socle ne fait que charger un modèle et l'optimiser pour la machine qui l'accueille. L'écran **« Édition & optimisation de modèle »**, ouvert depuis le catalogue de modèles, énumère ce qui marche réellement ici — rien n'est simulé :

| Disponible dans l'application | Ce que cela change |
| :--- | :--- |
| **Adaptateurs LoRA** | Charger des `.gguf` et régler leur intensité à chaud, sans redémarrage |
| **Quantification** | Choisir la précision (Q4/Q5/Q6/Q8) au téléchargement |
| **Compression du cache KV** | Cache 4/8-bit : plus de contexte à VRAM égale |
| **Offload RAM / experts MoE** | Des modèles plus gros que la VRAM disponible |
| **Décodage spéculatif** | Un petit modèle draft accélère la génération |

### L'entraînement vient d'un morph

`llama.cpp` ne fait que de l'inférence : pas de rétropropagation, pas de distillation, pas de chirurgie des poids. Ces opérations demandent une pile Python (Unsloth, PEFT, transformer-lens), et c'est le morph **[`morph-model-training`](https://github.com/Locaryn/morph-model-training)** (capacité `model-training`) qui apporte le studio :

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     ENTRAÎNEMENT & OBLITÉRATION                         │
├────────────────────────────────────┬────────────────────────────────────┤
│ 🚀 Fine-Tuning LoRA / QLoRA        │ 🔓 Oblitération de Refus (RepE)    │
│ • Datasets JSONL / Parquet         │ • Méthode RepE (Ablation vecteur)  │
│ • Taux d'apprentissage & Epochs    │ • Ciblage des couches de tenseurs  │
│ • Merge & Export GGUF immédiat     │ • Dé-alignement de recherche       │
└────────────────────────────────────┴────────────────────────────────────┘
```

Installé, le morph ajoute son entrée de menu, son onglet « Entraînement & LoRA » dans le Studio et son catalogue d'adaptateurs ; retiré, tout disparaît. La version publiée expose aujourd'hui l'API d'entraînement **sans moteur embarqué** : les appels échouent explicitement au lieu de fabriquer un résultat — l'application ne simule jamais une opération.

> [!IMPORTANT]
> **Avertissement de responsabilité** : L'oblitération de refus est destinée aux chercheurs et praticiens en IA pour étudier l'alignement des modèles. Une modale de consentement explicite rappelle que l'utilisateur est seul responsable de l'usage des poids modifiés.

---

## 🚀 Démarrage Rapide

### Prérequis
- **Rust** 1.88+
- **Node.js** 22+ & **pnpm** 9+

### Installation & Lancement en Développement

```bash
# 1. Cloner le dépôt
git clone https://github.com/Locaryn/Locaryn.git
cd Locaryn

# 2. Installer les dépendances
pnpm install

# 3. Lancer le service local (Daemon)
cargo run -p locaryn-daemon

# 4. Dans un second terminal : lancer l'application Desktop
pnpm tauri:dev
```

### Lanceur 1-Clic sous Windows
Un script complet prêt à l'emploi est disponible dans [`scripts/dev.bat`](scripts/dev.bat) :
```cmd
scripts\dev.bat
```

---

## 📥 Téléchargements & Releases

Retrouvez tous les paquets précompilés sur la page **[GitHub Releases](https://github.com/Locaryn/Locaryn/releases)** :

| Plateforme | Format | Description |
| :--- | :--- | :--- |
| **Windows x64 / arm64** | `Locaryn_<version>_x64-setup.exe` / `_arm64-setup.exe` | Installeur NSIS (signé, avec auto-update) |
| **Linux x64** | `Locaryn_<version>_amd64.deb` / `.AppImage` | Paquets Debian, Ubuntu et exécutable autonome |
| **macOS** | `Locaryn_<version>_universal.dmg` | Binaire universel (Apple Silicon & Intel) |
| **Android** | `locaryn-<version>-android.apk` | Application mobile compagne |
| **Serveur Headless** | `locaryn-<version>-server-*` (`locaryn-daemon`, `locaryn-cli`) | Démon, CLI et serveur distant (Windows / macOS / Linux) |

---

## 🤝 Contribution & Communauté

Les contributions sont les bienvenues ! Pour proposer une fonctionnalité, un morph, un skill ou un correctif :
1. Forkez le dépôt.
2. Créez votre branche (`git checkout -b feature/ma-fonctionnalite`).
3. Vérifiez la qualité avec `pnpm format && cargo test --workspace`.
4. Ouvrez une **Pull Request**.

---

<div align="center">

Licence **Apache-2.0** • Conçu et développé avec passion par la communauté Locaryn.

</div>
