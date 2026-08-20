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

[✨ Points Forts](#-pourquoi-locaryn--points-forts-et-avantages-clés) •
[🏛 Architecture](#-architecture-unifiée) •
[🧩 Extensions & Noyau](#-écosystème-dextensions--plugins-qui-modifient-le-noyau) •
[🧠 Modèles & Oblitération](#-studio-de-modèles-entraînement--oblitération-repe) •
[🚀 Démarrage Rapide](#-démarrage-rapide) •
[📥 Téléchargements](#-téléchargements--releases)

</div>

---

## 📖 Présentation

**Locaryn** est une plateforme complète d'intelligence artificielle locale et distribuée, conçue pour unifier le développement assisté par IA, la gestion et modification de modèles (GGUF, Ollama, API distantes), l'orchestration multi-agents et la collaboration multi-appareils.

Bâtie autour d'un **noyau partagé 100% Rust**, Locaryn combine une application de bureau native ultra-rapide (Tauri v2 + React), une CLI ergonomique, un serveur sécurisé (daemon headless) et une application mobile compagne (Android/iOS).

---

## ⚡ Pourquoi Locaryn ? — Points Forts et Avantages Clés

Contrairement aux solutions cloisonnées ou dépendantes du cloud propriétaire, Locaryn apporte une flexibilité inédite :

### 1. 🔄 Rôle Dual Client ⇄ Serveur Universel
- **Chaque client peut être serveur, et chaque serveur peut être client** : Utilisez votre PC fixe équipé d'un GPU comme serveur local ou distant, et pilotez-le en toute transparence depuis votre PC portable, votre tablette ou votre smartphone.
- **Appairage Instantané et Sécurisé** : Rejoignez une machine en scannant un QR code ou via le **Mode Découverte** sur le réseau local Wi-Fi, avec chiffrement de bout en bout (mTLS avec certificats auto-générés ou tunnels distants chiffrés).
- **Mode Éphémère & Vie Privée** : Vos échanges et sessions peuvent être basculés en mode éphémère d'un clic pour garantir une confidentialité totale sans enregistrement résiduel.

### 2. 🧩 Extensibilité Totale du Noyau (Core Extensibility)
- Les plugins dans Locaryn ne sont pas de simples gadgets : ils ont la capacité de **modifier le comportement même du noyau (core runtime)**, d'enregistrer des serveurs MCP natifs, d'intercepter les événements du système (hooks) et d'injecter des **vues et menus complets dans l'interface graphique** (ex: *Figures Académiques*, *Studio de Création*, *Fine-tuning & Oblitération*).
- **Compatibilité Universelle Cross-Écosystème** : Locaryn exécute et adapte nativement les extensions et skills provenant de **Locaryn**, **Claude Code**, **Gemini CLI**, **OpenCode** et du **Model Context Protocol (MCP)**. Un simple lien GitHub (`github:owner/repo`) installe n'importe quel plugin ou skill en un clic.

### 3. 🧠 Du Débutant au Chercheur / Power-User
- **Pour tous les utilisateurs** : Une interface moderne, épurée et réactive avec auto-détection des modèles locaux (GGUF, Ollama, vLLM, LM Studio) et des clés API (Claude, OpenAI, Gemini, DeepSeek, Mistral, OpenRouter).
- **Pour les créateurs et chercheurs** :
  - **Fine-Tuning & LoRA** : Entraînement et adaptation de modèles locaux.
  - **Oblitération RepE (Representation Engineering)** : Analyse et ablation directionnelle des couches de refus pour tester et dé-aligner les modèles de recherche en toute souveraineté (avec avertissement explicite de responsabilité).
  - **Figures & Visualisation** : Génération de diagrammes académiques et figures scientifiques de haute qualité.
  - **Espaces de travail distants (SSH)** : Connexion à des clusters et serveurs distants directement depuis vos conversations.

### 4. 🚀 Performance & Sobriété
- **100% Rust** : Zéro surcharge Electron, empreinte mémoire minimale, démarrage instantané et zéro télémétrie non sollicitée.

---

## 📊 Matrice Comparative

| Fonctionnalité | Locaryn | LM Studio / Ollama | Claude Desktop | Cursor / OpenCode |
| :--- | :---: | :---: | :---: | :---: |
| **Noyau Natif Rust ultra-léger** | ✅ Oui | ❌ Non / Partiel | ❌ Non (Electron) | ❌ Non |
| **Client ⇄ Serveur Interchangeable** | ✅ Oui (Desktop/Mobile/CLI) | ⚠️ Serveur local seul | ❌ Non | ❌ Non |
| **Appairage Mobile (QR / Découverte Wi-Fi)** | ✅ Oui | ❌ Non | ❌ Non | ❌ Non |
| **Plugins modifiant l'UI & le Noyau** | ✅ Oui (Deep Extensibility) | ❌ Non | ⚠️ MCP seul | ⚠️ Extensions limitées |
| **Compatibilité Claude / Gemini / MCP / OpenCode** | ✅ 100% universel | ❌ Non | ⚠️ MCP seul | ⚠️ Partiel |
| **Fine-Tuning & Oblitération RepE intégrée** | ✅ Oui (via plugin) | ❌ Non | ❌ Non | ❌ Non |
| **Génération Multimédia (Image, TTS, 3D, Figures)** | ✅ Oui (Plugins dédiés) | ❌ Non | ❌ Non | ❌ Non |
| **Mode Éphémère instantané** | ✅ Oui | ❌ Non | ❌ Non | ❌ Non |

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
└────────────────────────────────────┬────────────────────────────────────┘
                                     │
┌────────────────────────────────────▼────────────────────────────────────┐
│                      MOTEURS & FOURNISSEURS IA                          │
│   GGUF / Llama.cpp │ Ollama │ LM Studio │ Claude │ OpenAI │ Gemini │ SSH │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 🧩 Écosystème d'Extensions & Plugins Officiels

Locaryn dispose d'une collection d'extensions officielles modulaires prêtes à l'emploi :

| Plugin | Description | Capacités injectées |
| :--- | :--- | :--- |
| **`plugin-model-training`** | Fine-tuning LoRA, quantification GGUF et **Oblitération RepE (Décensure)** | Menu principal *Entraînement & Oblitération* |
| **`plugin-figures`** | Création de diagrammes, schémas vectoriels et figures scientifiques | Menu *Figures* & Rôles spécialisés |
| **`plugin-image-gen`** | Génération et édition d'images locales et distantes (Stable Diffusion, FLUX) | Studio d'images, commandes `/image` |
| **`plugin-image-editor`** | Retouche visuelle, inpainting et masquage d'images | Outils de manipulation d'images |
| **`plugin-3d-gen`** | Modélisation et génération d'objets et scènes 3D | Studio 3D, export GLB/OBJ |
| **`plugin-voice-tts`** | Synthèse vocale réaliste et lecture audio des réponses | Voix off, lecture vocale |
| **`plugin-dictaphone`** | Transcription vocale automatique (Whisper) et dictée | Saisie vocale, transcription audio |
| **`plugin-vision-ocr`** | Reconnaissance optique de caractères et analyse de documents | Extraction de texte, analyse visuelle |
| **`plugin-rag-qa`** | Indexation vectorielle, recherche sémantique et RAG local | Indexation de documents, Q&R |
| **`plugin-ssh`** | Gestionnaire de terminaux et connexions serveurs distants | Navigation SSH, exécution distante |
| **`plugin-travel-tunnel`** | Tunnels chiffrés pour accès extérieur (Cloudflare, ngrok, devtunnel) | Mode Voyage / Remote |
| **`plugin-translation`** | Traduction neuronale multi-langues de haute fidélité | Outils de traduction |

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
l'exemple complet, avec binaire, panneau et catalogue, est
[`plugin-image-gen`](https://github.com/Locaryn/plugin-image-gen).

---

## 🧠 Studio de Modèles : Entraînement & Oblitération RepE

Le plugin **`plugin-model-training`** intègre une suite d'outils avancés pour les ingénieurs et chercheurs :

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
| **Windows x64** | `Locaryn_0.3.12_x64-setup.exe` | Installeur NSIS (Recommandé avec auto-update) |
| **Linux x64** | `Locaryn_0.3.12_amd64.deb` / `.AppImage` | Paquets Debian, Ubuntu et exécutable autonome |
| **macOS** | `Locaryn_0.3.12_universal.dmg` | Binaire universel (Apple Silicon M1/M2/M3 & Intel) |
| **Android** | `Locaryn_0.3.12_android.apk` | Application mobile compagne |
| **Serveur Headless** | `locaryn-daemon` & `locaryn-cli` | Démon et CLI pour serveurs Linux/Windows |

---

## 🤝 Contribution & Communauté

Les contributions sont les bienvenues ! Pour proposer une fonctionnalité, un plugin ou un correctif :
1. Forkez le dépôt.
2. Créez votre branche (`git checkout -b feature/ma-fonctionnalite`).
3. Vérifiez la qualité avec `pnpm format && cargo test --workspace`.
4. Ouvrez une **Pull Request**.

---

<div align="center">

Licence **Apache-2.0** • Conçu et développé avec passion par la communauté Locaryn.

</div>
