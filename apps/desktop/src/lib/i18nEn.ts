// Traductions anglaises de l'interface, générées depuis les textes français
// du code (extraction par l'arbre TypeScript) puis traduites à la main.
// Les clés sont le texte français normalisé (espaces resserrés, entités HTML
// décodées). Les noms de modèles, de marques et les termes techniques n'y
// figurent pas : ils ne se traduisent jamais.
//
// `EN_PATTERNS` couvre les textes construits avec des variables
// (`Téléchargement de ${nom}`) : chaque « {} » est une valeur gardée telle quelle.

export const EN_EXACT: Record<string, string> = {
  "Paramètres système avancés": "Advanced system settings",
  "Découvrir et installer d'autres modèles": "Discover and install more models",
  "Choisir, supprimer, ouvrir le dossier": "Choose, delete, open the folder",
  "Marketplace d'extensions": "Extension marketplace",
  "Ajouter des morphs et des skills": "Add morphs and skills",
  Interface: "Interface",
  "Le niveau de détail affiché. Rien n'est supprimé en mode simplifié : tout reste accessible.":
    "How much detail is shown. Nothing is removed in simplified mode: everything stays reachable.",
  Simplifiée: "Simplified",
  Complète: "Full",
  "L'essentiel seulement. Catalogues et réglages techniques rangés dans les paramètres avancés.":
    "Essentials only. Catalogues and technical settings are tucked into advanced settings.",
  "Toute l'application : modèles, extensions, terminal et réglages à portée de main.":
    "The whole app: models, extensions, terminal and settings within reach.",
  "Journaux et rapports": "Logs and reports",
  "En cas de problème, les journaux disent ce qui s'est passé. Votre dossier personnel, vos clés et vos jetons y sont masqués.":
    "When something goes wrong, the logs say what happened. Your home folder, keys and tokens are masked.",
  "Voir les journaux": "View logs",
  "Masquer les journaux": "Hide logs",
  "Journaux copiés : collez-les dans votre message (Discord, e-mail…).":
    "Logs copied: paste them into your message (Discord, email…).",
  "Envoyer à l'équipe": "Send to the team",
  "Journaux envoyés à l'équipe Locaryn. Merci !": "Logs sent to the Locaryn team. Thank you!",
  "Envoyer automatiquement les rapports d'erreur": "Send error reports automatically",
  "Un plantage, un moteur qui ne démarre pas ou une conversation qui échoue envoient le résumé de l'erreur et les journaux (masqués) à l'équipe Locaryn, pour corriger plus vite. Jamais vos conversations ni vos fichiers.":
    "A crash, an engine that won't start or a failed conversation sends the error summary and the (masked) logs to the Locaryn team so it can be fixed faster. Never your conversations or files.",
  "Distribution par défaut": "Default distribution",
  "Décisions rapides des morphs": "Fast decisions for morphs",
  "Un petit modèle local aide le contrôle du bureau et du navigateur à choisir où cliquer et à vérifier le résultat, en une fraction de seconde, sans occuper le modèle de conversation. Il s'installe tout seul avec ces morphs (environ 1,3 Go, sur le disque de vos modèles).":
    "A small local model helps desktop and browser control choose where to click and check the result in a fraction of a second, without tying up the chat model. It installs itself with those morphs (about 1.3 GB, on your models drive).",
  "Installé (processeur)": "Installed (CPU)",
  "Installé (carte NVIDIA)": "Installed (NVIDIA GPU)",
  "Non installé": "Not installed",
  "Installation en cours…": "Installing…",
  Réinstaller: "Reinstall",
  "Téléchargement de l'installateur (uv)…": "Downloading the installer (uv)…",
  "Installation de Python…": "Installing Python…",
  "Installation de PyTorch (processeur)…": "Installing PyTorch (CPU)…",
  "Installation de PyTorch (CUDA)…": "Installing PyTorch (CUDA)…",
  "Installation du modèle de décision…": "Installing the decision model…",
  "Téléchargement du modèle de décision…": "Downloading the decision model…",
  "Téléchargement d'image": "Image download",
  "Charger quand même": "Load anyway",
  "Prudence actuelle :": "Current caution:",
  "Fichier créé ou modifié": "File created or modified",
  "Fichiers créés ou modifiés": "Files created or modified",
  "(1 échec)": "(1 failure)",
  "(Connecté)": "(Connected)",
  "(Installé localement)": "(Installed locally)",
  "(Installé)": "(Installed)",
  "(K-Quant adaptatif) compresse intelligemment chaque couche selon son importance, préservant la précision sur l'attention critique.":
    "(adaptive K-Quant) smartly compresses each layer according to its importance, preserving precision on critical attention.",
  "(abliteré). Les filtres de sécurité sont retirés.": "(abliterated). Safety filters are removed.",
  "(aucun outil n'a été appelé de tout l'objectif : le modèle n'a fait qu'écrire du texte)":
    "(no tool was called during the whole goal: the model only wrote text)",
  "(aucune différence de texte)": "(no text difference)",
  "(aucune sortie)": "(no output)",
  "(chargement des couches une par une — un GPU 4 Go de VRAM suffit)":
    "(layers loaded one at a time — a 4 GB VRAM GPU is enough)",
  "(installé)": "(installed)",
  "(jamais demandée)": "(never requested)",
  "(journal vide)": "(empty log)",
  "(les outils appelés ont répondu sans erreur)": "(the tools called answered without error)",
  "(nomic-embed, bge, e5) — un modèle de chat sépare mal les sens.":
    "(nomic-embed, bge, e5) — a chat model separates meanings poorly.",
  "(refusée)": "(refused)",
  "(rien — aucun message système n'est envoyé)": "(nothing — no system message is sent)",
  "(sans limite)": "(no limit)",
  "(sans message)": "(no message)",
  ") depuis GitHub. Le moteur de conversation, lui, est déjà natif ARM64 et utilise le GPU Adreno.":
    ") from GitHub. The chat engine itself is already native ARM64 and uses the Adreno GPU.",
  ") ne suit pas le dossier de stockage : elle pèse quelques mégaoctets et reste ouverte pendant que l'application tourne, la déplacer à chaud risquerait de la corrompre.":
    ") does not follow the storage folder: it weighs a few megabytes and stays open while the app runs; moving it live could corrupt it.",
  ") ou lien HTTP direct vers un fichier": ") or a direct HTTP link to a file",
  ") sur le modèle. L'entraînement se fait en Python (Unsloth/PEFT) puis":
    ") on the model. Training is done in Python (Unsloth/PEFT) then",
  ") utilisent une matrice de calibration pour préserver un maximum de cohérence tout en réduisant drastiquement le poids.":
    ") use a calibration matrix to preserve as much coherence as possible while drastically reducing size.",
  "+ Ajouter un connecteur MCP personnalisé…": "+ Add a custom MCP connector…",
  "+ Ajouter un emplacement SSH...": "+ Add an SSH location...",
  "+ Ajouter un projet": "+ Add a project",
  "+ Ajouter une marketplace": "+ Add a marketplace",
  "+ Ajouter une variable": "+ Add a variable",
  "+ Définir un identifiant administrateur": "+ Set an administrator login",
  "+ Installer un Morph ou un Skill": "+ Install a Morph or a Skill",
  "+ Nouveau": "+ New",
  "+ Nouvelle session projet": "+ New project session",
  "+ Ouvrir un dossier local...": "+ Open a local folder...",
  ", arrêtée sur demande.": ", stopped on request.",
  ", build Vulkan — GPU NVIDIA/AMD/Intel). Pas de service externe à assembler.":
    ", Vulkan build — NVIDIA/AMD/Intel GPU). No external service to set up.",
  ", et joignable depuis cette machine seulement.": ", and reachable from this machine only.",
  ", ils n'injectent pas d'écrans ou de composants graphiques dans l'application hôte.":
    ", they do not inject screens or graphical components into the host app.",
  ", l'état se met à jour tout seul.": ", the status updates by itself.",
  ", pendant les téléchargements de modèles. Créez-le sur":
    ", during model downloads. Create it on",
  ", puis proposer les variantes GGUF compatibles et leur projecteur vision.":
    ", then offer the compatible GGUF variants and their vision projector.",
  ", qui refusent les téléchargements anonymes. Le token est envoyé uniquement à huggingface.co, dans l'en-tête":
    ", which refuse anonymous downloads. The token is only sent to huggingface.co, in the header",
  ", un dossier local ou une archive": ", a local folder or an archive",
  ", une URL GitHub (y compris": ", a GitHub URL (including",
  ", ÷4 VRAM), activée avec Flash Attention. Léger impact sur la qualité aux très longs contextes.":
    ", ÷4 VRAM), enabled with Flash Attention. Slight quality impact at very long contexts.",
  ". Ajoutez-en une par": ". Add one per",
  ". Laisse vide pour rester en local.": ". Leave empty to stay local.",
  ". Le format est détecté automatiquement : plugin Locaryn, plugin Claude Code, extension Gemini CLI, paquet OpenCode ou":
    ". The format is detected automatically: Locaryn plugin, Claude Code plugin, Gemini CLI extension, OpenCode package or",
  ". Les modèles ne sont jamais fournis avec l'app : vous les téléchargez, et chacun garde sa propre licence.":
    ". Models never ship with the app: you download them, and each keeps its own license.",
  ". Ouvrez son dossier dans « Mes modèles » pour la coller — sans elle, le choix sera refusé.":
    ". Open its folder in “My models” to paste it — without it, the choice will be refused.",
  ". Rien ne tourne avant que vous l'ayez relu et approuvé.":
    ". Nothing runs until you have reviewed and approved it.",
  ". Sans compression, le modèle ne verrait plus le début du fil.":
    ". Without compression, the model would no longer see the beginning of the thread.",
  ".txt .md .csv .json .html, code source… — plusieurs fichiers acceptés":
    ".txt .md .csv .json .html, source code… — several files accepted",
  "/var/www/mon-projet ou /root": "/var/www/my-project or /root",
  "0 couche sur le GPU": "0 layers on the GPU",
  "1 serveur sera ajouté.": "1 server will be added.",
  "192.168.1.100 ou srv.domaine.com": "192.168.1.100 or srv.domain.com",
  "412 étoiles": "412 stars",
  "= Large (précision renforcée).": "= Large (higher precision).",
  "= Medium (recommandé),": "= Medium (recommended),",
  "= Small (légèrement allégé),": "= Small (slightly lighter),",
  ACTIF: "ACTIVE",
  "AUCUNE EXTENSION": "NO EXTENSION",
  "AVIS DE RESPONSABILITÉ & SÉCURITÉ :": "LIABILITY & SAFETY NOTICE:",
  "AWQ / EXL2 / GPTQ (nécessite ExLlamaV2 / vLLM)": "AWQ / EXL2 / GPTQ (requires ExLlamaV2 / vLLM)",
  "Abliteration (retrait des refus)": "Abliteration (refusal removal)",
  "Accès réseau ou machine sans modification d'UI": "Network or machine access without UI changes",
  "Accéder au Marketplace de Modèles": "Open the Model Marketplace",
  "Accélérer la génération avec un petit modèle draft.":
    "Speed up generation with a small draft model.",
  Actif: "Active",
  "Actif : tous les modèles deviennent exécutables — les modèles trop lourds pour ce PC tournent en local via AirLLM (chargement couche par couche, ex. Kimi K3 sur un GPU 4 Go de VRAM).":
    "On: every model becomes runnable — models too heavy for this PC run locally through AirLLM (layer by layer, e.g. Kimi K3 on a 4 GB VRAM GPU).",
  "Actions sur la sélection": "Actions on selection",
  Activer: "Enable",
  "Activer le canal de mise à jour Bêta de l'application": "Enable the app's Beta update channel",
  "Activer les garde-fous": "Enable safeguards",
  Actualiser: "Refresh",
  "Adaptateurs LoRA chargés": "Loaded LoRA adapters",
  "Adresse copiée": "Address copied",
  "Adresse de connexion (Mobile / Web)": "Connection address (Mobile / Web)",
  "Adresse du serveur": "Server address",
  "Adresse du serveur (ex: https://192.168.1.50:7474)":
    "Server address (e.g. https://192.168.1.50:7474)",
  "Adresse ou recherche": "Address or search",
  "Affichage en Liste Détaillée": "Detailed list view",
  Afficher: "Show",
  "Afficher :": "Show:",
  "Afficher Tous les Modèles": "Show all models",
  "Afficher le code en grand": "Show code full size",
  "Afficher le token": "Show token",
  "Afficher les Morphs en version Bêta / Non testée": "Show Beta / untested Morphs",
  "Afficher les Morphs en version Bêta dans le catalogue": "Show Beta Morphs in the catalog",
  "Afficher les messages en attente": "Show queued messages",
  "Afficher les suivants": "Show next ones",
  "Afficher uniquement les modèles NSFW / sans garde-fous connus":
    "Only show NSFW / known unguarded models",
  "Afficher uniquement les modèles classiques avec garde-fous":
    "Only show standard models with safeguards",
  "Afficher uniquement les modèles réentraînables via Fine-Tuning / LoRA":
    "Only show models trainable via Fine-Tuning / LoRA",
  "Afficher uniquement les modèles sans garde-fous / oblitérés":
    "Only show unguarded / obliterated models",
  "Agent autonome et système de mémoire persistante":
    "Autonomous agent and persistent memory system",
  Aide: "Help",
  "AirLLM — Gros modèles sur petit GPU": "AirLLM — Big models on a small GPU",
  Ajouter: "Add",
  "Ajouter au contexte": "Add to context",
  "Ajouter ce PC": "Add this PC",
  "Ajouter depuis un dépôt": "Add from a repository",
  "Ajouter un PC": "Add a PC",
  "Ajouter un connecteur MCP": "Add an MCP connector",
  "Ajouter un débridage": "Add an unlock",
  "Ajouter un emplacement SSH": "Add an SSH location",
  "Ajouter un emplacement de travail SSH": "Add an SSH workspace",
  "Ajouter un modèle": "Add a model",
  "Ajouter un test pour ce composant": "Add a test for this component",
  "Ajouter une marketplace": "Add a marketplace",
  "Ajouter/retirer prend effet au prochain démarrage du serveur (changement de modèle ou premier message).":
    "Adding/removing takes effect on the next server start (model change or first message).",
  "Ajoutez ce qu'il faut savoir pour y travailler.": "Add what one needs to know to work on it.",
  "Ajustez vos composants matériels pour identifier les modèles utilisables sur votre ordinateur.":
    "Adjust your hardware to find the models usable on your computer.",
  "Alternative 5-bit équilibrée.": "Balanced 5-bit alternative.",
  "Analyse PC requise pour estimer": "PC scan required to estimate",
  "Analyse de la source…": "Analyzing source…",
  "Analyse de type précise et autocomplétion pour le code du projet.":
    "Precise type analysis and autocompletion for the project's code.",
  "Analyse des fichiers et des quantifications HuggingFace…":
    "Analyzing HuggingFace files and quantizations…",
  "Analyse du PC en cours… la compatibilité de chaque modèle s'affichera automatiquement.":
    "Scanning the PC… each model's compatibility will show automatically.",
  "Analyse visuelle et compréhension graphique multimodale.":
    "Visual analysis and multimodal image understanding.",
  "Analyser le code existant": "Analyze existing code",
  "Analyser le dossier et écrire LOCARYN.md, le contexte que le modèle relira":
    "Analyze the folder and write LOCARYN.md, the context the model will reread",
  "Analyser les composants de mon PC pour adapter les recommandations":
    "Scan my PC's hardware to tailor recommendations",
  "Analyseur de Performances & Compatibilité Matérielle":
    "Performance & Hardware Compatibility Analyzer",
  Annuler: "Cancel",
  "Annuler (": "Cancel (",
  "Annuler le téléchargement en cours": "Cancel the current download",
  Aperçu: "Preview",
  "Aperçu HTML": "HTML preview",
  "Aperçu SVG": "SVG preview",
  "Aperçu indisponible pour cette source :": "Preview unavailable for this source:",
  "Appareil refusé. Son code ne vaut plus rien : affichez un nouveau QR pour recommencer.":
    "Device refused. Its code is no longer valid: show a new QR to start again.",
  Apparence: "Appearance",
  "Appels d'outils qui ont échoué :": "Tool calls that failed:",
  "Applique des adaptateurs LoRA (fichiers": "Applies LoRA adapters (files",
  "Applique/permute des LoRA .gguf — l'entraînement se fait en Python":
    "Applies/swaps .gguf LoRAs — training is done in Python",
  "Appliquer la modification": "Apply the change",
  "Appliquer le filtre des modèles recommandés (": "Apply the recommended models filter (",
  "Apporté par": "Provided by",
  "Approbations d'outil": "Tool approvals",
  "Approuver et ajouter": "Approve and add",
  "Après cette réponse": "After this answer",
  "Architecture MoE 47B paramètres (8 experts) tournant couche par couche sur 4 Go de VRAM.":
    "47B-parameter MoE architecture (8 experts) running layer by layer on 4 GB of VRAM.",
  Archiver: "Archive",
  "Archiver le projet": "Archive project",
  "Archiver retire le projet de la liste ; ses conversations restent sur le disque et rien n'est supprimé.":
    "Archiving removes the project from the list; its conversations stay on disk and nothing is deleted.",
  "Archiver «": "Archive “",
  "Arreter la generation": "Stop generation",
  Arrêter: "Stop",
  Arrêté: "Stopped",
  "Arrêté.": "Stopped.",
  "Arrêt…": "Stopping…",
  Aucun: "None",
  "Aucun adaptateur préchargé.": "No preloaded adapter.",
  "Aucun adaptateur. Ajoutez-en depuis Paramètres → Moteur IA.":
    "No adapter. Add some from Settings → AI Engine.",
  "Aucun catalogue en cache. Lancez « Actualiser » pour lire les marketplaces Claude Code, l'index Gemini CLI et les plugins OpenCode publiés sur npm. Les serveurs MCP sont classés dans « Connecteurs & MCP ».":
    "No cached catalog. Click “Refresh” to read the Claude Code marketplaces, the Gemini CLI index and the OpenCode plugins published on npm. MCP servers are listed under “Connectors & MCP”.",
  "Aucun certificat": "No certificate",
  "Aucun compte utilisateur configuré.": "No user account configured.",
  "Aucun connecteur MCP configuré": "No MCP connector configured",
  "Aucun document indexé": "No indexed document",
  "Aucun dossier spécifique": "No specific folder",
  "Aucun entraînement en cours...": "No training in progress...",
  "Aucun fichier de poids standard n'a été identifié. Ce dépôt semble être un paquet multi-fichiers (par exemple un modèle TTS). Installer le dépôt complet téléchargera aussi ses fichiers de configuration.":
    "No standard weight file was found. This repository looks like a multi-file package (a TTS model for instance). Installing the whole repository will also download its configuration files.",
  "Aucun fichier temporaire à supprimer.": "No temporary file to delete.",
  "Aucun lot pour l'instant — aucune soumission réelle n'est encore câblée.":
    "No batch yet — no real submission is wired yet.",
  "Aucun module de Studio actif": "No active Studio module",
  "Aucun modèle TTS détecté. Installez-en un depuis le Marketplace pour pouvoir le choisir.":
    "No TTS model detected. Install one from the Marketplace to choose it.",
  "Aucun modèle chargé": "No model loaded",
  "Aucun modèle chargé : ce réglage servira au prochain chargement.":
    "No model loaded: this setting will apply to the next load.",
  "Aucun modèle correspondant dans cette catégorie.": "No matching model in this category.",
  "Aucun modèle d'image détecté. Installez un checkpoint de diffusion depuis l'extension Génération d'images pour pouvoir le choisir.":
    "No image model detected. Install a diffusion checkpoint from the Image generation extension to choose it.",
  "Aucun modèle en mémoire": "No model in memory",
  "Aucun modèle installé localement": "No model installed locally",
  "Aucun modèle installé sur ce PC.": "No model installed on this PC.",
  "Aucun modèle installé.": "No model installed.",
  "Aucun modèle local actuellement installé :": "No local model currently installed:",
  "Aucun modèle n'est installé sur ce PC.": "No model is installed on this PC.",
  "Aucun modèle ne correspond à vos filtres.": "No model matches your filters.",
  "Aucun modèle ne correspond à votre recherche": "No model matches your search",
  "Aucun modèle ne correspond.": "No matching model.",
  "Aucun moteur n'est configuré : ouvrez Réglages → Moteur avant d'installer un modèle.":
    "No engine configured: open Settings → Engine before installing a model.",
  "Aucun outil ne correspond.": "No matching tool.",
  "Aucun paquet de cette famille ne correspond au filtre sélectionné.":
    "No package of this family matches the selected filter.",
  "Aucun projet. Ajoutez-en un depuis la barre latérale pour configurer ses autorisations.":
    "No project. Add one from the sidebar to configure its permissions.",
  "Aucun résultat pour cette recherche.": "No results for this search.",
  "Aucun résultat.": "No results.",
  "Aucun skill indexé pour ce noyau.": "No skill indexed for this core.",
  "Aucun — ne rien nommer automatiquement": "None — don't name anything automatically",
  Aucune: "None",
  "Aucune archive ne correspond à «": "No archive matches “",
  "Aucune archive à rechercher pour le moment.": "No archive to search yet.",
  "Aucune clé enregistrée pour": "No key saved for",
  "Aucune clé à coller. Ouvrez pour la démarrer et choisir un modèle.":
    "No key to paste. Open it to start it and choose a model.",
  "Aucune commande de lancement déclarée : démarrez le noyau vous-même, Locaryn s'y connectera ensuite.":
    "No launch command declared: start the core yourself, Locaryn will then connect to it.",
  "Aucune conversation": "No conversation",
  "Aucune conversation active.": "No active conversation.",
  "Aucune conversation archivée. Glissez-en une sur la corbeille pour la ranger ici.":
    "No archived conversation. Drag one onto the trash to file it here.",
  "Aucune conversation ne correspond à votre recherche.": "No conversation matches your search.",
  "Aucune conversation ouverte. Envoyez un premier message : elle est créée, et ses permissions se règlent ici. Les nouvelles conversations démarrent avec le réglage de Réglages → Compte.":
    "No open conversation. Send a first message: it gets created, and its permissions are set here. New conversations start with the setting from Settings → Account.",
  "Aucune conversation récente à afficher.": "No recent conversation to show.",
  "Aucune description fournie pour ce paquet.": "No description provided for this package.",
  "Aucune exception : cette conversation suit le réglage du projet, ou celui des nouvelles conversations.":
    "No exception: this conversation follows the project's setting, or the one for new conversations.",
  "Aucune extension installée ne revendique ces poids. Ils occupent de la place sans servir.":
    "No installed extension claims these weights. They take up space without being used.",
  "Aucune extension, plugin ou noyau installé. Ouvrez « Découvrir » pour parcourir les catalogues, ou installez un paquet depuis un dépôt.":
    "No extension, plugin or core installed. Open “Discover” to browse catalogs, or install a package from a repository.",
  "Aucune fiche pour ce projet.": "No note for this project.",
  "Aucune figure. Écrivez-en une : un nom, et ce que le modèle doit faire.":
    "No figure. Write one: a name, and what the model should do.",
  "Aucune mise à jour": "No update",
  "Aucune modification depuis le dernier commit.": "No change since the last commit.",
  "Aucune note de version publiée.": "No release notes published.",
  "Aucune pour l'instant.": "None yet.",
  "Aucune source enregistrée — impossible de mettre à jour.": "No saved source — cannot update.",
  "Aucune source n'a répondu — affichage de la dernière copie locale.":
    "No source answered — showing the last local copy.",
  "Aucune tâche": "No task",
  "Aucune tâche pour l'instant.": "No task yet.",
  "Authentification OAuth GitLab pour OpenCode.": "GitLab OAuth authentication for OpenCode.",
  "Automatique — premier modèle TTS installé": "Automatic — first installed TTS model",
  "Automatique — premier modèle de diffusion installé":
    "Automatic — first installed diffusion model",
  "Autorisation demandée": "Permission requested",
  "Autorisations des outils": "Tool permissions",
  "Autorise l'agent à effectuer des recherches en ligne pour la documentation du projet.":
    "Lets the agent search online for project documentation.",
  "Autoriser et activer": "Allow and enable",
  "Autoriser l'affichage et l'installation des Morphs en version préliminaire ou non vérifiés (marqués en ambre) dans le store.":
    "Allow showing and installing pre-release or unverified Morphs (marked in amber) in the store.",
  "Autoriser les notifications du système": "Allow system notifications",
  "Avancement dans la barre des tâches": "Progress in the taskbar",
  Avancé: "Advanced",
  "Avatar personnalisé": "Custom avatar",
  "Avis de Responsabilité & Cadre Légal :": "Liability Notice & Legal Framework:",
  "Bande passante mémoire non mesurable en démonstration, 48 Go/s supposés.":
    "Memory bandwidth cannot be measured in demo, 48 GB/s assumed.",
  "Basculer le moteur AirLLM : les modèles trop lourds pour ce PC deviennent exécutables localement (chargement des couches une par une, un GPU 4 Go de VRAM suffit)":
    "Toggle the AirLLM engine: models too heavy for this PC become runnable locally (layers loaded one at a time, a 4 GB VRAM GPU is enough)",
  "Base de données": "Database",
  "Base de données PostgreSQL ou MySQL": "PostgreSQL or MySQL database",
  Bibliothèque: "Library",
  "Bientôt disponible": "Coming soon",
  "Bêta · non testé": "Beta · untested",
  "C'est déjà le dossier utilisé.": "This is already the folder in use.",
  "C'est le cœur : ce que le modèle reçoit avant chaque conversation de cette figure. Écrivez ce qu'il doit faire, et surtout ce qu'il ne doit pas faire — une consigne négative tient mieux qu'un ton demandé.":
    "This is the core: what the model receives before each conversation of this figure. Write what it must do, and above all what it must not do — a negative instruction holds better than a requested tone.",
  "C'est le réglage des conversations de ce projet, sauf exception posée sur l'une d'elles.":
    "This is the setting for this project's conversations, unless one of them has an exception.",
  "C'est le texte exact d'une conversation où des outils sont offerts. S'il est vide ou ne contient que les règles d'outillage, alors un refus vient du modèle lui-même : c'est son entraînement qui parle, et seul un autre modèle y changera quelque chose.":
    "This is the exact text of a conversation where tools are offered. If it is empty or only contains the tool rules, a refusal comes from the model itself: its training is speaking, and only another model will change that.",
  "C'est une décision du projet, pas une préférence.":
    "This is a project decision, not a preference.",
  "CONTEXTE DU PROJET": "PROJECT CONTEXT",
  CONVERSATIONS: "CONVERSATIONS",
  "CPU uniquement — aucune couche sur GPU": "CPU only — no layer on the GPU",
  "Cache 4/8-bit pour allonger le contexte à VRAM égale.":
    "4/8-bit cache to extend the context for the same VRAM.",
  "Cache 4/8-bit pour contexte long à VRAM égale": "4/8-bit cache for long context at equal VRAM",
  "Cache Q8 (÷2 VRAM, sans perte visible)": "Q8 cache (÷2 VRAM, no visible loss)",
  "Calcul et soustraction du vecteur de refus...":
    "Computing and subtracting the refusal vector...",
  "Calcule l'attention par blocs : moins de mémoire, plus rapide sur les longs contextes. Nécessaire pour compresser le cache. À laisser activé.":
    "Computes attention in blocks: less memory, faster on long contexts. Required to compress the cache. Keep it on.",
  "Calcule le vecteur de direction de refus d'un modèle **déjà installé** et applique une projection orthogonale pour neutraliser les filtres.":
    "Computes the refusal direction vector of an **already installed** model and applies an orthogonal projection to neutralize filters.",
  "Canal de mise à jour Bêta (Testeurs)": "Beta update channel (Testers)",
  "Capacité aux outils non vérifiée": "Tool support not verified",
  "Capacités :": "Capabilities:",
  "Capacités du moteur": "Engine capabilities",
  "Caractère du modèle": "Model personality",
  "Catalogue de modèles": "Model catalog",
  "Catalogue des connecteurs": "Connector catalog",
  "Ce LoRA/embedding est classé NSFW / sans garde-fous. Je prends la responsabilité de son usage.":
    "This LoRA/embedding is classified NSFW / unguarded. I take responsibility for its use.",
  "Ce certificat prouve à quelle machine appartient cette installation.":
    "This certificate proves which machine this installation belongs to.",
  "Ce connecteur est arrêté : la liste de ses outils vient de lui, il faut le démarrer pour la voir.":
    "This connector is stopped: its tool list comes from it, start it to see the list.",
  "Ce dossier n'est pas un dépôt git : il n'y a pas d'historique à comparer.":
    "This folder is not a git repository: there is no history to compare.",
  "Ce dépôt Transformers complet sera supprimé définitivement, y compris tous ses shards Safetensors.":
    "This whole Transformers repository will be permanently deleted, including all its Safetensors shards.",
  "Ce modèle": "This model",
  "Ce modèle est actuellement débridé (sans filtre ni censure)":
    "This model is currently unlocked (no filter or censorship)",
  "Ce modèle est classé NSFW / sans garde-fous. Acceptez la responsabilité avant de télécharger. [demo]":
    "This model is classified NSFW / unguarded. Accept responsibility before downloading. [demo]",
  "Ce modèle est trop lourd pour les composants de ce PC (":
    "This model is too heavy for this PC's hardware (",
  "Ce nom et votre avatar sont conservés sur cette machine et restaurés au prochain démarrage.":
    "This name and your avatar are kept on this machine and restored at next startup.",
  "Ce nom préfixe les outils vus par le modèle : lettres, chiffres, « - » et « _ » uniquement.":
    "This name prefixes the tools the model sees: letters, digits, “-” and “_” only.",
  "Ce pour quoi Locaryn vous dérange": "What Locaryn interrupts you for",
  "Ce qu'il faut savoir pour travailler ici": "What to know to work here",
  "Ce que Locaryn peut appliquer à un modèle en local, et ce qui demande une pile d'entraînement séparée. Rien n'est simulé ici.":
    "What Locaryn can apply to a model locally, and what needs a separate training stack. Nothing is simulated here.",
  "Ce que Locaryn retient de vous": "What Locaryn remembers about you",
  "Ce que le modele pourra faire dans chaque nouveau chat. Les conversations deja ouvertes gardent les leurs, et chacune reste modifiable depuis son panneau de reglages.":
    "What the model may do in each new chat. Conversations already open keep theirs, and each stays editable from its settings panel.",
  "Ce que le modèle garde en tête : messages, fichiers lus, outils. Auto = toute la mémoire vidéo laissée libre par le modèle. Plus grand = plus de VRAM ; si elle manque, des couches passent sur le processeur et tout ralentit.":
    "What the model keeps in mind: messages, files read, tools. Auto = all the video memory the model leaves free. Larger = more VRAM; if it runs short, layers move to the CPU and everything slows down.",
  "Ce que le modèle peut faire": "What the model can do",
  "Ce que le modèle peut faire dans les nouveaux chats": "What the model can do in new chats",
  "Ce qui a changé dans le dépôt, fichier par fichier.":
    "What changed in the repository, file by file.",
  "Ce qui tourne en arrière-plan : sous-agents, plans, téléchargements, générations.":
    "What runs in the background: sub-agents, plans, downloads, generations.",
  "Ce réglage ne change pas le profil du moteur d'inférence.":
    "This setting does not change the inference engine profile.",
  "Ce texte ouvre chaque conversation — sauf les éphémères, qui repartent toujours de rien.":
    "This text opens every conversation — except ephemeral ones, which always start from scratch.",
  "Celle du message reçu": "The one of the received message",
  "Celui déjà chargé — le modèle de la conversation":
    "The one already loaded — the conversation's model",
  "Certains serveurs n'acceptent que les postes qu'ils ont eux-mêmes autorisés. Le fichier « .pem » transmis par votre administrateur s'installe ici.":
    "Some servers only accept machines they authorized themselves. The “.pem” file from your administrator is installed here.",
  "Certificat de connexion": "Connection certificate",
  "Certificat installé": "Certificate installed",
  "Certificat installé.": "Certificate installed.",
  "Certificat retiré.": "Certificate removed.",
  "Ces commandes modifient une demande : écrivez-la à la suite, par exemple « /plan répare le parseur ».":
    "These commands modify a request: write it after them, for example “/plan fix the parser”.",
  "Ces poids seront supprimés définitivement. L'extension qui les utilise ne les retrouvera pas.":
    "These weights will be permanently deleted. The extension using them will not find them again.",
  "Cet espace regroupe votre identité, vos réglages de compte et vos conversations archivées. Aucun compte en ligne n'est nécessaire pour utiliser Locaryn.":
    "This space gathers your identity, your account settings and your archived conversations. No online account is needed to use Locaryn.",
  "Cet identifiant (nom d'utilisateur et mot de passe) sera requis pour vous connecter depuis l'application mobile ou l'interface web.":
    "This login (username and password) will be required to connect from the mobile app or the web interface.",
  "Cette action est bloquée par le niveau de permission de la conversation":
    "This action is blocked by the conversation's permission level",
  "Cette architecture n'est pas encore supportée par AirLLM (Llama, Mistral, Qwen2…)":
    "This architecture is not yet supported by AirLLM (Llama, Mistral, Qwen2…)",
  "Cette conversation": "This conversation",
  "Cette conversation est confiée à un noyau alternatif : sa mémoire, ses skills et son fournisseur lui appartiennent.":
    "This conversation is handled by an alternative core: its memory, skills and provider belong to it.",
  "Cette conversation pèse déjà ~": "This conversation already weighs ~",
  "Cette extension déclare son propre appairage : son QR ne passe pas par les segments natifs du panneau de droite.":
    "This extension declares its own pairing: its QR does not go through the native segments of the right panel.",
  "Cette extension ne déclare aucun connecteur MCP modifiable.":
    "This extension declares no editable MCP connector.",
  "Cette extension ne déclare aucun réglage. Rien à configurer ici.":
    "This extension declares no setting. Nothing to configure here.",
  "Cette figure lit ce que le service retient de vous":
    "This figure reads what the service remembers about you",
  "Cette machine est ARM64 : la version x64 tourne sous émulation, plus lentement. Installez la version Windows ARM64 (":
    "This machine is ARM64: the x64 version runs under emulation, more slowly. Install the Windows ARM64 version (",
  "Change la langue des textes de l'application. Les noms de modèles, de marques et les termes techniques restent inchangés.":
    "Changes the language of the app's texts. Model names, brands and technical terms stay unchanged.",
  "Changer de Modèle Installé": "Switch installed model",
  "Changer de dossier…": "Change folder…",
  "Changer de modèle": "Change model",
  "Changer la portée exige l'application Locaryn, pas le navigateur.":
    "Changing the scope requires the Locaryn app, not the browser.",
  "Changer sans déplacer": "Change without moving",
  "Chargement des préférences…": "Loading preferences…",
  "Chargement du modèle en mémoire…": "Loading the model into memory…",
  "Chargement du modèle…": "Loading the model…",
  "Chargement rapide, moins de RAM copiée": "Fast loading, less RAM copied",
  "Charger des .gguf et régler leur intensité à chaud, sans redémarrage.":
    "Load .gguf files and tune their strength live, without restart.",
  "Charger et garder en mémoire": "Load and keep in memory",
  "Charger un modèle": "Load a model",
  "Charger un modèle en mémoire": "Load a model into memory",
  "Chat / outils": "Chat / tools",
  "Chat et agent": "Chat and agent",
  "Checkpoint / modèle NSFW ou sans garde-fous": "NSFW or unguarded checkpoint / model",
  "Chemin vers le modèle draft (ex: models/gemma-2b.gguf)":
    "Path to the draft model (e.g. models/gemma-2b.gguf)",
  "Chemin vers un adaptateur .gguf": "Path to a .gguf adapter",
  "Cherche les commentaires FIXME et TODO": "Find FIXME and TODO comments",
  "Chercher sur HuggingFace Hub": "Search HuggingFace Hub",
  "Chercher un GGUF": "Search for a GGUF",
  "Choisir la précision (Q4/Q5/Q6/Q8) au téléchargement dans le marketplace.":
    "Choose the precision (Q4/Q5/Q6/Q8) when downloading from the marketplace.",
  "Choisir le modèle à installer": "Choose the model to install",
  "Choisir parmi les modèles installés": "Choose among installed models",
  "Choisir un avatar": "Choose an avatar",
  "Choisir un dossier sur cette machine": "Choose a folder on this machine",
  "Choisir un dossier…": "Choose a folder…",
  "Choisir un modèle": "Choose a model",
  "Choisir un modèle parmi ceux installés": "Choose a model among installed ones",
  "Choisir un modèle à charger en mémoire": "Choose a model to load into memory",
  "Choisir un projet": "Choose a project",
  "Choisir une archive ZIP…": "Choose a ZIP archive…",
  "Choisissez ici les modèles utilisés par les tâches secondaires de Locaryn. Le modèle de conversation reste configurable depuis le chat.":
    "Choose here the models used by Locaryn's secondary tasks. The conversation model stays configurable from the chat.",
  "Choisissez une image PNG, JPG, WEBP ou BMP. Locaryn en conserve une copie locale, même si l'original est déplacé.":
    "Choose a PNG, JPG, WEBP or BMP image. Locaryn keeps a local copy, even if the original is moved.",
  Clair: "Light",
  "Cliquez pour reprendre les coordonnées d'une connexion passée. « Oublier » efface aussi le mot de passe mémorisé, s'il y en avait un.":
    "Click to reuse the details of a past connection. “Forget” also erases the remembered password, if any.",
  "Clé API (my.telegram.org)": "API key (my.telegram.org)",
  "Clé effacée.": "Key erased.",
  "Clé enregistrée": "Key saved",
  "Clé enregistrée dans le trousseau du système.": "Key saved in the system keychain.",
  "Clé enregistrée. Ouvrez pour choisir un modèle.": "Key saved. Open to choose a model.",
  "Clé obtenue": "Key obtained",
  "Cochez d'abord la case de confirmation": "Tick the confirmation box first",
  "Cochez des conversations": "Tick conversations",
  "Cochez les connecteurs auxquels ce projet a le droit d'accéder durant les sessions de chat.":
    "Tick the connectors this project may access during chat sessions.",
  "Code QR à faire scanner": "QR code to scan",
  "Code de confirmation — à saisir sur le téléphone": "Confirmation code — enter it on the phone",
  "Code de connexion": "Connection code",
  Coller: "Paste",
  "Coller du JSON": "Paste JSON",
  "Collez ici le texte à indexer…": "Paste the text to index here…",
  "Collez le bloc": "Paste the block",
  "Collez votre clé": "Paste your key",
  "Collez votre clé pour utiliser vos modèles payants ici.":
    "Paste your key to use your paid models here.",
  "Combien le modèle doit réfléchir avant de répondre":
    "How much the model should think before answering",
  "Commande par défaut": "Default command",
  "Commande à lancer": "Command to run",
  "Commandes pour le flux de commit git.": "Commands for the git commit flow.",
  Compatibilité: "Compatibility",
  "Composants Matériels de votre Machine :": "Your Machine's Hardware:",
  "Comprendre les suffixes (_K_M, _0, IQ, Q6_K vs Q8_0) ▼":
    "Understand the suffixes (_K_M, _0, IQ, Q6_K vs Q8_0) ▼",
  "Compresser la conversation": "Compress the conversation",
  "Compresser maintenant ? Les anciens échanges deviennent un résumé concis, les derniers restent intacts.":
    "Compress now? Older exchanges become a concise summary, the latest stay intact.",
  "Compression de la conversation": "Conversation compression",
  "Compression du cache KV": "KV cache compression",
  Compte: "Account",
  "Compte & Profil": "Account & Profile",
  "Compte local autonome — aucune connexion distante configurée.":
    "Standalone local account — no remote connection configured.",
  "Compte relié à un serveur Locaryn.": "Account linked to a Locaryn server.",
  "Compte rendu du sous-agent": "Sub-agent report",
  "Compte ·": "Account ·",
  "Compte → Archives": "Account → Archives",
  "Configuration du Fine-Tuning LoRA": "LoRA Fine-Tuning setup",
  "Configuration générale de votre profil, de vos Morphs, de vos compétences et du moteur d'inférence.":
    "General setup of your profile, your Morphs, your skills and the inference engine.",
  "Configure comment le modèle est exécuté sur ta machine":
    "Configure how the model runs on your machine",
  "Configurés (": "Configured (",
  "Confirmation de responsabilité": "Responsibility confirmation",
  "Confirmations demandées, action par action": "Confirmations asked, action by action",
  "Confirmer le mot de passe": "Confirm password",
  "Connecteur MCP · arrêté": "MCP connector · stopped",
  "Connecteurs MCP déclarés (": "Declared MCP connectors (",
  "Connectez cette application à un serveur ou supercalculateur IA distant (ex: DGX Spark).":
    "Connect this app to a remote AI server or supercomputer (e.g. DGX Spark).",
  Connexion: "Connection",
  "Connexion de": "Connection from",
  "Connexion distante (facultative)": "Remote connection (optional)",
  "Connexion en cours…": "Connecting…",
  "Connexion à l'autre PC…": "Connecting to the other PC…",
  "Connexions SSH, bases de données et serveurs MCP": "SSH connections, databases and MCP servers",
  "Connexion…": "Connecting…",
  "Conseillé pour votre carte": "Recommended for your card",
  "Console du Script d'Oblitération": "Obliteration Script Console",
  "Contexte = mémoire vidéo restante (8K au moins)":
    "Context = remaining video memory (at least 8K)",
  "Contexte compressé": "Context compressed",
  "Continuez… le contexte se compresse au bout": "Keep holding… the context compresses at the end",
  "Contrôle de Home Assistant : états, scènes, automatisations.":
    "Home Assistant control: states, scenes, automations.",
  Conversation: "Conversation",
  "Conversation compressée.": "Conversation compressed.",
  "Conversation libre sans dossier assigné": "Free conversation with no assigned folder",
  "Conversation locale": "Local conversation",
  "Conversation éphémère": "Ephemeral conversation",
  "Conversation éphémère — rien n'en sera gardé : ni le fil, ni son titre.":
    "Ephemeral conversation — nothing will be kept: neither the thread nor its title.",
  Conversations: "Conversations",
  "Conversations (": "Conversations (",
  "Conversations libres": "Free conversations",
  "Conversations rangées": "Filed conversations",
  "Conversations réunies": "Merged conversations",
  Copier: "Copy",
  "Copier l'adresse": "Copy address",
  "Copier l'adresse du lien": "Copy link address",
  "Copier l'image": "Copy image",
  "Copier le chemin": "Copy path",
  "Copier le détail de l'erreur": "Copy error details",
  "Copier le lien": "Copy link",
  "Copier le message": "Copy message",
  Copié: "Copied",
  "Couches déportées sur le GPU": "Layers offloaded to the GPU",
  "Couleur d'accentuation, thème": "Accent color, theme",
  "Couleur personnalisée": "Custom color",
  Couper: "Cut",
  "Création…": "Creating…",
  "Créer l'identifiant": "Create login",
  "Créer l'identifiant et démarrer le serveur": "Create the login and start the server",
  "Créer la figure": "Create figure",
  "Créer une clé": "Create a key",
  "Créer, lire ou modifier un classeur Excel.": "Create, read or edit an Excel workbook.",
  "Créer, lire ou modifier un document Word (.docx).":
    "Create, read or edit a Word document (.docx).",
  "Créer, lire ou modifier une présentation PowerPoint.":
    "Create, read or edit a PowerPoint presentation.",
  "Cœurs CPU": "CPU cores",
  "Date, heure et conversions de fuseaux horaires.": "Date, time and time zone conversions.",
  "Demande de connexion à un serveur": "Server connection request",
  "Depuis l'application Locaryn du téléphone, bouton « Scanner un QR code » — pas avec l'appareil photo, qui ne sait pas le lire. Le code porte l'adresse et l'empreinte de cette machine ; le code de confirmation n'apparaît ici qu'une fois le téléphone arrivé sur ce serveur.":
    "From the Locaryn app on the phone, “Scan a QR code” button — not with the camera app, which cannot read it. The code carries this machine's address and fingerprint; the confirmation code only shows here once the phone has reached this server.",
  "Diagnostic d'Exécution avec cette Configuration :": "Run diagnosis with this configuration:",
  "Disponible dans l'application": "Available in the app",
  "Disponible dans l'application de bureau.": "Available in the desktop app.",
  "Documentation à jour des bibliothèques, servie en MCP distant.":
    "Up-to-date library documentation, served as a remote MCP.",
  "Documents indexés utilisés automatiquement dans les conversations de ce projet.":
    "Indexed documents used automatically in this project's conversations.",
  "Donnez vos documents au modèle une bonne fois pour toutes. Ensuite, à":
    "Give your documents to the model once and for all. Then, at",
  "Données déplacées. Le nouveau dossier est actif.": "Data moved. The new folder is active.",
  Dossier: "Folder",
  "Dossier de données": "Data folder",
  "Dossier de stockage": "Storage folder",
  "Dossier de travail": "Working folder",
  "Dossier vide.": "Empty folder.",
  "Durée minimale d'une tâche": "Minimum task duration",
  "Débit estimé": "Estimated speed",
  Débridé: "Unlocked",
  "Décharger le modèle et rendre la mémoire": "Unload the model and free the memory",
  "Décoché, aucune bannière ne part. Le centre de notifications de l'application continue de tout enregistrer.":
    "Unticked, no banner is sent. The app's notification center keeps recording everything.",
  "Décoché, l'assistant répond à tout sans limite ni escalade.":
    "Unticked, the assistant answers everything with no limit or escalation.",
  "Décochée, elle travaille à part : elle ne sait rien de vos préférences ni de vos projets, et ne répondra qu'à partir de ses consignes.":
    "Unticked, it works apart: it knows nothing of your preferences or projects, and answers only from its instructions.",
  "Décodage Spéculatif": "Speculative Decoding",
  "Décodage spéculatif": "Speculative decoding",
  "Découvert automatiquement sur le Hub HuggingFace":
    "Discovered automatically on the HuggingFace Hub",
  "Découverte et installation de modèles locaux & HuggingFace":
    "Discover and install local & HuggingFace models",
  "Découverte sur le réseau": "Network discovery",
  Découvrir: "Discover",
  "Découvrir les derniers modèles en direct": "Discover the latest models live",
  "Décrire le projet": "Describe the project",
  "Décrivez ce qu'il faut changer dans le champ ci-dessous, ou ouvrez une fiche pour la corriger directement.":
    "Describe what to change in the field below, or open a note to fix it directly.",
  "Décrivez librement comment l'assistant doit se comporter.":
    "Describe freely how the assistant should behave.",
  "Définir un identifiant de connexion": "Set a login",
  "Définir un serveur et dossier distant pour le chat":
    "Set a remote server and folder for the chat",
  "Démarrage…": "Starting…",
  "Démarrage en cours…": "Starting…",
  "Cacher les modèles non utilisables": "Hide models not usable",
  "Démarre le serveur qui expose les fonctions de Locaryn — modèles, conversations et outils — aux appareils autorisés sur votre réseau (application mobile, interface web).":
    "Starts the server that exposes Locaryn's features — models, conversations and tools — to authorized devices on your network (mobile app, web interface).",
  Démarrer: "Start",
  "Démarrer automatiquement avec l'application": "Start automatically with the app",
  "Démarrer et afficher les outils": "Start and show tools",
  "Démarrer la passerelle": "Start the gateway",
  "Démarrer maintenant après l'ajout": "Start now after adding",
  "Démarrer une conversation éphémère (rien ne sera conservé)":
    "Start an ephemeral conversation (nothing will be kept)",
  "Démarré, mais ce serveur n'annonce aucun outil.": "Started, but this server announces no tool.",
  "Dépasse la mémoire de ce PC — non recommandé": "Exceeds this PC's memory — not recommended",
  "Déplacer les données": "Move the data",
  "Déplacer vers": "Move to",
  "Déposer ici pour archiver": "Drop here to archive",
  "Déposez vos fichiers": "Drop your files",
  "Dépôt GitHub contenant": "GitHub repository containing",
  Désactiver: "Disable",
  "Désactivé : le modèle reçoit d'un coup toutes les commandes des connecteurs actifs. Activé : il reçoit d'abord une ligne par connecteur — son nom, ce qu'il fait, combien de commandes — et charge un connecteur entier quand la demande le désigne. Les connecteurs cités avec « @ » restent toujours chargés, et les outils intégrés aussi. Utile avec les petits modèles, qui se perdent dans des listes de commandes. Pour retirer un outil précis, utilisez « Configurer » sur son connecteur.":
    "Off: the model receives every tool of the active connectors and knows what it has. On: when tools weigh too much in its window, their descriptions are shortened and those unrelated to the request are set aside — useful for small models. In every case, tools bigger than the window itself are trimmed, and the app says so. To remove a specific tool, use “Configure” on its connector.",
  Désinstaller: "Uninstall",
  Détail: "Detail",
  "Détail de la fenêtre de contexte": "Context window details",
  Détails: "Details",
  "Détails et portée": "Details and scope",
  "Détermine ce que l'agent peut faire seul dans ce projet (lire, écrire, exécuter).":
    "Decides what the agent can do on its own in this project (read, write, run).",
  "Effacer l'index de ce projet ?": "Erase this project's index?",
  "Effacer la recherche": "Clear search",
  Emplacement: "Location",
  "Emplacement des modèles, espace disque, nettoyage": "Model location, disk space, cleanup",
  "Empreinte du certificat": "Certificate fingerprint",
  "En attente du scan…": "Waiting for the scan…",
  "En mode risqué, aucun chargement n'est refusé. Un modèle trop gros pour la mémoire fera compenser le système sur le disque : ralentissement sévère, et l'application peut être tuée par manque de mémoire.":
    "In risky mode, no load is refused. A model too big for memory will make the system swap to disk: severe slowdown, and the app may be killed for lack of memory.",
  "En mémoire, déchargement automatique": "In memory, automatic unloading",
  "En mémoire, épinglé": "In memory, pinned",
  "En pause : le modèle attend votre réponse": "Paused: the model is waiting for your answer",
  "En pratique : des modèles": "In practice: models",
  "En une ligne": "In one line",
  Enregistrer: "Save",
  "Enregistrer et démarrer": "Save and start",
  "Enregistrer la connexion": "Save connection",
  "Enregistrer le profil": "Save profile",
  "Enregistrer les connecteurs MCP": "Save MCP connectors",
  "Enregistrer les paramètres du projet": "Save project settings",
  "Enregistrer sans activer": "Save without enabling",
  "Enregistrer sous": "Save as",
  "Enregistrer sous…": "Save as…",
  Enregistré: "Saved",
  "Enregistré.": "Saved.",
  Enregistrée: "Saved",
  "Entraînement & Modification de Modèles": "Model Training & Modification",
  "Entraînement LoRA / QLoRA": "LoRA / QLoRA training",
  "Entraînement LoRA/QLoRA, distillation, pruning (pile Python : Unsloth/PEFT)":
    "LoRA/QLoRA training, distillation, pruning (Python stack: Unsloth/PEFT)",
  "Entraînement en cours...": "Training in progress...",
  "Entraînez des adaptateurs LoRA ou neutralisez les filtres de refus sur vos modèles locaux installés.":
    "Train LoRA adapters or neutralize refusal filters on your installed local models.",
  Entrée: "Enter",
  "Entrée pour envoyer · Shift+Entrée pour saut de ligne":
    "Enter to send · Shift+Enter for a new line",
  "Environnement de chat principal, exécution de code et prompts":
    "Main chat environment, code execution and prompts",
  "Environnement de travail du serveur distant": "Remote server workspace",
  Envoyer: "Send",
  "Envoyer ce message maintenant": "Send this message now",
  "Envoyer cette suite": "Send this follow-up",
  "Envoyer le message": "Send the message",
  "Envoyer maintenant": "Send now",
  "Envoyer ↵": "Send ↵",
  Erreur: "Error",
  "Erreur copiée": "Error copied",
  "Estimation AirLLM sur ce PC — débit réel selon VRAM / RAM / disque":
    "AirLLM estimate on this PC — real speed depends on VRAM / RAM / disk",
  "Et ensuite ?": "What's next?",
  "Everything (démo)": "Everything (demo)",
  "Excellente qualité compressée par calibration.":
    "Excellent quality, compressed through calibration.",
  "Exception posée sur cette conversation ; les autres chats ne bougent pas.":
    "Exception set on this conversation; other chats are unchanged.",
  "Experts des N premières couches sur le CPU (-ncmoe N)":
    "Experts of the first N layers on the CPU (-ncmoe N)",
  "Expiré : affichez un nouveau QR.": "Expired: show a new QR.",
  "Explique la structure de ce projet": "Explain this project's structure",
  "Extension démo": "Demo extension",
  Extensions: "Extensions",
  "Extensions & Skills": "Extensions & Skills",
  "Extensions et connecteurs autorisés pour ce projet":
    "Extensions and connectors allowed for this project",
  "Externe (non supervisé)": "External (not supervised)",
  Extrême: "Extreme",
  "Exécute du code Luau dans Studio.": "Runs Luau code in Studio.",
  "Exécuter cette action": "Run this action",
  "Exécuter des modèles plus gros que la VRAM disponible.":
    "Run models bigger than the available VRAM.",
  "Exécuter et travailler dans l'environnement du serveur distant":
    "Run and work in the remote server environment",
  "Exécutez un bloc de code depuis une réponse : la sortie du terminal ou la page rendue s'affichera ici.":
    "Run a code block from an answer: the terminal output or the rendered page will show here.",
  "Faites calculer le modèle par un PC plus puissant de votre réseau, en gardant tout le reste ici.":
    "Have a more powerful PC on your network compute the model, keeping everything else here.",
  "Famille MoE multimodale de Meta (texte + image). Scout (109B/17B actifs) et Maverick (400B/17B actifs).":
    "Meta's multimodal MoE family (text + image). Scout (109B/17B active) and Maverick (400B/17B active).",
  "Famille flagship multilingue (119+ langues), mode thinking hybride, dense et MoE. Apache 2.0.":
    "Multilingual flagship family (119+ languages), hybrid thinking mode, dense and MoE. Apache 2.0.",
  "Famille multimodale de dernière génération (texte, image, audio). Architecture encoder-free unifiée avec function calling natif.":
    "Latest-generation multimodal family (text, image, audio). Unified encoder-free architecture with native function calling.",
  "Fenêtre de contexte": "Context window",
  Fermer: "Close",
  "Fermer ce panneau": "Close this panel",
  "Fermer ce suivi": "Close this tracker",
  "Fermer l'alerte": "Close the alert",
  "Fermer l'espace de travail": "Close the workspace",
  "Fermer l'image agrandie": "Close the enlarged image",
  "Fermer l'onglet": "Close tab",
  "Fermer la fenêtre la place dans la zone de notification et conserve le service actif. Utilisez":
    "Closing the window puts it in the notification area and keeps the service running. Use",
  "Fermer le détail": "Close details",
  "Fermer le détail du contexte": "Close context details",
  "Fermer le menu": "Close menu",
  "Fermer les paramètres": "Close settings",
  "Fermer les paramètres du projet": "Close project settings",
  "Fiche oubliée": "Note forgotten",
  "Fichier Batch au format JSONL (1 requête par ligne avec custom_id) :":
    "Batch file in JSONL format (1 request per line with custom_id):",
  "Fichier Dataset (.jsonl / .txt)": "Dataset file (.jsonl / .txt)",
  "Fichier binaire : il ne s'affiche pas ici.": "Binary file: it is not shown here.",
  "Fichier de poids du modèle.": "Model weight file.",
  "Fichier de session": "Session file",
  Fichiers: "Files",
  "Fichiers Protégés & Interdits à l'IA (Patterns Glob)":
    "Protected files forbidden to the AI (Glob patterns)",
  "Fichiers temporaires": "Temporary files",
  "Fidélité maximale, requiert plus de RAM.": "Maximum fidelity, needs more RAM.",
  "Filtrer les outils": "Filter tools",
  "Filtrer les paquets installés": "Filter installed packages",
  "Filtrer mes modèles installés…": "Filter my installed models…",
  "Filtrer par année de sortie": "Filter by release year",
  "Filtrer uniquement les modèles adaptés aux composants de votre PC":
    "Only show models suited to your PC's hardware",
  "Fin ou échec du téléchargement d'un modèle.": "End or failure of a model download.",
  "Flagship 70B de Meta exécutable en local sur machine modeste via le streaming de couches AirLLM.":
    "Meta's 70B flagship runnable locally on a modest machine through AirLLM layer streaming.",
  "FlexGen : streaming SSD orchestré (mmap seul = lent)":
    "FlexGen: orchestrated SSD streaming (mmap alone = slow)",
  "Fluidité Maximale": "Maximum smoothness",
  "Force une sortie JSON valide": "Forces a valid JSON output",
  "Forcer quand même": "Force anyway",
  "Forcer un workflow": "Force a workflow",
  "Format étranger, entièrement déclaratif : converti sans perte à l'installation.":
    "Foreign format, fully declarative: converted losslessly at install.",
  "Formats acceptés : tag Ollama (": "Accepted formats: Ollama tag (",
  Français: "Français",
  "GPU du moteur": "Engine GPU",
  "Garde les experts d'un modèle MoE (GLM, Qwen3-MoE, DeepSeek) en RAM et l'attention sur le GPU. Fait tourner d'énormes modèles sur une petite carte, bien plus vite que le streaming SSD.":
    "Keeps a MoE model's experts (GLM, Qwen3-MoE, DeepSeek) in RAM and attention on the GPU. Runs huge models on a small card, much faster than SSD streaming.",
  "Garde une petite réserve de VRAM ; accepte qu'une partie du modèle passe par la RAM si elle a de la place.":
    "Keeps a small VRAM reserve; accepts part of the model going through RAM if it has room.",
  "Garder le fil tronque": "Keep the truncated thread",
  "Glissez vos documents ici, ou cliquez pour choisir":
    "Drag your documents here, or click to choose",
  "Go. Exécution GPU ultra-rapide.": "GB. Ultra-fast GPU execution.",
  "Go. Nécessite une extension de mémoire.": "GB. Needs a memory upgrade.",
  "Guide de choix de la quantification": "Quantization selection guide",
  Génération: "Generation",
  "Génération d'images": "Image generation",
  "Génération de texte + appels d'outils (agent)": "Text generation + tool calls (agent)",
  "Gérer les adaptateurs": "Manage adapters",
  "Gérer les modèles": "Manage models",
  "Gérer vos modèles locaux, ouvrir le dossier et sélection rapide":
    "Manage your local models, open the folder and quick selection",
  "Haute fidélité (Q5/Q6/Q8)": "High fidelity (Q5/Q6/Q8)",
  "Hauteur du panneau du bas": "Bottom panel height",
  "Historique des conversations": "Conversation history",
  "Historique des lots": "Batch history",
  "Historique et conversations récentes": "History and recent conversations",
  "Hors de portée de ce moteur": "Out of this engine's reach",
  "HuggingFace, Ollama ou lien direct .gguf": "HuggingFace, Ollama or direct .gguf link",
  "Hybrid Mamba2-Transformer MoE (31.6B total, ~3.6B actifs) pour workflows agentiques à 1M tokens.":
    "Hybrid Mamba2-Transformer MoE (31.6B total, ~3.6B active) for 1M-token agentic workflows.",
  "Hôte / IP": "Host / IP",
  "Identifiant sur l'autre PC": "Login on the other PC",
  "Identifiants configurés (": "Configured logins (",
  "Identité, avatar, connexion": "Identity, avatar, connection",
  "Idéal pour faire tourner un gros modèle avec peu de mémoire.":
    "Ideal to run a big model with little memory.",
  "Idéal si vous avez 16-32 Go de mémoire.": "Ideal if you have 16-32 GB of memory.",
  "Idéal si vous êtes un peu juste en mémoire.": "Ideal if you are a bit short on memory.",
  "Ignorer cette notification": "Dismiss this notification",
  "Il nomme les conversations d'après leur sujet, pour qu'une liste se lise. Par défaut, c'est le modèle déjà chargé. Avec « Aucun », le titre reste la première phrase.":
    "It names conversations after their topic, so a list reads well. By default, it is the model already loaded. With “None”, the title stays the first sentence.",
  "Ils seront joints au message : images, PDF, textes, code…":
    "They will be attached to the message: images, PDFs, texts, code…",
  Image: "Image",
  "Image agrandie": "Enlarged image",
  "Image en cours de création": "Image being created",
  "Image, vidéo, audio, musique, 3D et édition multimodale":
    "Image, video, audio, music, 3D and multimodal editing",
  Images: "Images",
  "Importer ZIP…": "Import ZIP…",
  "Importer des fichiers": "Import files",
  "Importer un Morph ou un Skill directement depuis une archive ZIP":
    "Import a Morph or a Skill directly from a ZIP archive",
  "Imposer un skill ou un outil pour cette demande": "Force a skill or a tool for this request",
  "Impossible de charger cette note.": "Cannot load this note.",
  "Inactif : seuls les modèles tenant dans ce PC sont proposés en téléchargement local.":
    "Off: only models that fit this PC are offered for local download.",
  "Indexer des documents (RAG)": "Index documents (RAG)",
  "Indiquez ce qu'il faut modifier ou supprimer…": "Say what to change or delete…",
  "Inférence distribuée": "Distributed inference",
  "Inférence distribuée (RPC)": "Distributed inference (RPC)",
  "Inférence possible via partage RAM/CPU jusqu'à": "Inference possible via RAM/CPU sharing up to",
  "Installation puis démarrage…": "Installing then starting…",
  "Installation à autoriser": "Installation to authorize",
  Installer: "Install",
  "Installer cette variante": "Install this variant",
  "Installer des modèles dans le Marketplace": "Install models from the Marketplace",
  "Installer et démarrer": "Install and start",
  "Installer le dépôt complet": "Install the whole repository",
  "Installer le runtime": "Install the runtime",
  "Installer sur cet appareil": "Install on this device",
  "Installer sur cet appareil demande l'application de bureau.":
    "Installing on this device requires the desktop app.",
  "Installer un Morph, un Skill ou un Noyau depuis un dépôt, un dossier ou une archive":
    "Install a Morph, a Skill or a Core from a repository, a folder or an archive",
  "Installer une extension": "Install an extension",
  "Installer une extension (dépôt, dossier ou ZIP)":
    "Install an extension (repository, folder or ZIP)",
  "Installer…": "Install…",
  "Installez une extension officielle ou communautaire pour ajouter un module au Studio.":
    "Install an official or community extension to add a module to the Studio.",
  Installé: "Installed",
  Installée: "Installed",
  "Installée par Locaryn dans": "Installed by Locaryn in",
  "Intensité Alpha (": "Alpha strength (",
  "Intensité d'Ablation (Alpha :": "Ablation strength (Alpha:",
  "Intensité en direct (serveur en cours — sans redémarrage) :":
    "Live strength (server running — no restart):",
  Intermédiaire: "Balanced",
  "Interrompt la generation sans decharger le modele":
    "Interrupts generation without unloading the model",
  Intégré: "Built-in",
  "J'accepte la décharge et confirme utiliser cette option pour la cybersécurité légale.":
    "I accept the waiver and confirm I use this option for lawful cybersecurity.",
  "JSON des connecteurs MCP": "MCP connectors JSON",
  "JSON invalide — attendu un objet dont les valeurs sont des chaînes.":
    "Invalid JSON — expected an object whose values are strings.",
  'JSON invalide — attendu un objet, ex. { "API_KEY": "…" }':
    'Invalid JSON — expected an object, e.g. { "API_KEY": "…" }',
  "Je certifie utiliser cette fonctionnalité dans un cadre légal de recherche ou de pentest autorisé.":
    "I certify I use this feature within a lawful research or authorized pentest framework.",
  "Je comprends que cette action est irréversible et peut modifier le système":
    "I understand this action is irreversible and may modify the system",
  "Je suis conscient·e et j'accepte la responsabilité": "I am aware and accept responsibility",
  "Je vais noter ce format dans le contexte du projet. Selon votre choix, il suivra le projet, votre compte, ou restera sur cet ordinateur.":
    "I will note this format in the project context. Depending on your choice, it will follow the project, your account, or stay on this computer.",
  "Jeton d'accès": "Access token",
  "Jetons lus d'un coup quand le modèle lit votre message, l'historique et les outils. Plus grand = lecture plus rapide des longues demandes, un peu plus de VRAM. Ne change pas la vitesse à laquelle il écrit.":
    "Tokens read at once when the model reads your message, the history and the tools. Larger = faster reading of long requests, a bit more VRAM. Does not change how fast it writes.",
  Joindre: "Attach",
  "Joindre au message": "Attach to message",
  "Joindre des fichiers au message": "Attach files to the message",
  "Joindre uniquement des fichiers. Le modèle chargé ne sait pas analyser d'images — il lui faudrait un fichier « mmproj » à côté de ses poids ; chargez un modèle multimodal pour les joindre aussi.":
    "Attach files only. The loaded model cannot analyze images — it would need an “mmproj” file next to its weights; load a multimodal model to attach them too.",
  Journal: "Log",
  "Journaux d'Entraînement": "Training logs",
  "L'agent IA aura une interdiction absolue d'accéder, de lire ou de modifier les fichiers correspondant à ces motifs.":
    "The AI agent will be strictly forbidden to access, read or modify files matching these patterns.",
  "L'application n'est qu'un outil et": "The app is only a tool and",
  "L'hôte affiche maintenant un code à 6 chiffres, avec le nom de ce poste. Saisissez-le ici : il expire après 2 minutes.":
    "The host now shows a 6-digit code, with this machine's name. Enter it here: it expires after 2 minutes.",
  "L'icône de l'application porte une barre de progression : c'est le seul moyen de suivre un téléchargement sans rouvrir la fenêtre. Indépendant des bannières.":
    "The app icon carries a progress bar: the only way to follow a download without reopening the window. Independent of banners.",
  "L'oblitération des contraintes de refus est dédiée **exclusivement au pentesting d'infrastructure encadré et à la recherche en cybersécurité**.":
    "Refusal-constraint obliteration is meant **exclusively for supervised infrastructure pentesting and cybersecurity research**.",
  "La base (": "The base (",
  "La clé est vide.": "The key is empty.",
  "La conversation utilise de nouveau le modèle de ce PC.":
    "The conversation uses this PC's model again.",
  "La conversation, vos fichiers, le terminal, les MCP (Roblox Studio…)":
    "The conversation, your files, the terminal, MCPs (Roblox Studio…)",
  "La fenêtre est plus petite que la conversation": "The window is smaller than the conversation",
  "La génération n'a pas abouti.": "Generation did not complete.",
  "La mémoire de la conversation, en VRAM. Q8 la divise par deux sans perte visible : deux fois plus de contexte sur la même carte. Q4 la divise par quatre, avec un peu de précision en moins sur les longs échanges. La vitesse d'écriture ne change presque pas.":
    "The conversation's memory, in VRAM. Q8 halves it with no visible loss: twice the context on the same card. Q4 divides it by four, with a bit less precision on long exchanges. Writing speed barely changes.",
  "La mémoire est vide.": "Memory is empty.",
  "La mémoire libre est vérifiée avant chaque chargement, et le moteur garde la réserve choisie libre quand il dimensionne le contexte. Un modèle qui ne la respecte pas n'est pas chargé : une fenêtre l'explique.":
    "Free memory is checked before each load, and the engine keeps the chosen reserve free when it sizes the context. A model that doesn't respect it is not loaded: a window explains why.",
  "La note arrive…": "The note is coming…",
  "La passerelle sur votre machine": "The gateway on your machine",
  "La quantification compresse le modèle pour tourner localement avec fluidité sans saturer votre mémoire vive (RAM) ou carte graphique (VRAM).":
    "Quantization compresses the model so it runs smoothly locally without saturating your RAM or graphics card (VRAM).",
  "La soumission réelle vers un fournisseur n'est pas encore implémentée.":
    "Real submission to a provider is not implemented yet.",
  "La teinte unique de l'interface. Sobre et naturelle par défaut.":
    "The interface's single tint. Sober and natural by default.",
  "La version": "Version",
  "La vérification a échoué sans détail fourni par le système.":
    "The check failed with no detail from the system.",
  "Laisse le modèle décider": "Let the model decide",
  "Lancer avec AirLLM": "Run with AirLLM",
  "Lancer l'oblitération": "Start obliteration",
  "Lancer le Fine-Tuning LoRA": "Start LoRA Fine-Tuning",
  Langue: "Language",
  "Langue de l'interface": "Interface language",
  "Langue des réponses": "Answer language",
  "Largeur du panneau latéral": "Side panel width",
  "Le bouton vérifie si une version plus récente existe sur GitHub.":
    "The button checks whether a newer version exists on GitHub.",
  "Le catalogue complet. Chaque famille regroupe ses tailles, et chaque taille ses quantifications.":
    "The full catalog. Each family groups its sizes, and each size its quantizations.",
  "Le catalogue se met à jour automatiquement toutes les heures":
    "The catalog updates automatically every hour",
  "Le certificat est généré par cette machine, donc les postes clients afficheront un avertissement au premier contact. C'est attendu : cette empreinte est ce qui permet de vérifier qu'ils parlent bien à":
    "The certificate is generated by this machine, so client devices will show a warning on first contact. That's expected: this fingerprint lets them check they are really talking to",
  "Le changement de niveau depuis l'interface arrive avec le journal d'approbations ; les demandes d'outils restent confirmées au cas par cas d'ici là.":
    "Changing the level from the interface comes with the approval log; tool requests stay confirmed case by case until then.",
  "Le chargement ne respecte pas la prudence choisie dans Réglages → Performance. Choisissez une version plus légère du modèle, un contexte plus court, ou un niveau de prudence moins strict.":
    "The load does not respect the caution chosen in Settings → Performance. Pick a lighter version of the model, a shorter context, or a less strict caution level.",
  "Le dossier de la conversation, en lecture.": "The conversation folder, read-only.",
  "Le dépôt d'origine contient": "The original repository contains",
  "Le fil complet est dans l'onglet Tâches": "The full thread is in the Tasks tab",
  "Le modèle a été déchargé de la mémoire après une longue inactivité.":
    "The model was unloaded from memory after a long inactivity.",
  "Le modèle chargé ne sait pas appeler d'outils": "The loaded model cannot call tools",
  "Le modèle de code 32B spécialisé pour le développement logiciel long-horizon.":
    "The 32B code model specialized in long-horizon software development.",
  "Le modèle doit tenir entièrement dans la VRAM en laissant la réserve ci-dessous libre. Sinon, il n'est pas chargé.":
    "The model must fit entirely in VRAM while leaving the reserve below free. Otherwise, it is not loaded.",
  "Le modèle décide s'il faut un plan, et l'exécute le cas échéant":
    "The model decides whether a plan is needed, and runs it if so",
  "Le modèle enchaîne sans rendre la main jusqu'à ce que les conditions soient remplies":
    "The model keeps going without handing back until the conditions are met",
  "Le modèle joue un utilisateur qui essaie l'application, l'installation ou la page":
    "The model plays a user trying the app, the installation or the page",
  "Le modèle le lira à sa prochaine étape, sans attendre la fin":
    "The model will read it at its next step, without waiting for the end",
  "Le modèle n'aura plus à demander dans cette conversation, sauf pour installer quelque chose. La tâche continue sans interruption.":
    "The model will no longer need to ask in this conversation, except to install something. The task continues without interruption.",
  "Le modèle ne voit et n'appelle que les outils cochés. Décochez ceux qu'il ne doit pas pouvoir utiliser, par exemple ceux qui suppriment ou écrasent.":
    "The model only sees and calls the ticked tools. Untick those it must not use, for example those that delete or overwrite.",
  "Le modèle quitte la mémoire après 30 minutes sans utilisation. Un message, ou n'importe quelle action dans l'application, relance le décompte.":
    "The model leaves memory after 30 minutes without use. A message, or any action in the app, restarts the countdown.",
  "Le modèle raisonne davantage": "The model reasons more",
  "Le modèle réfléchit avant de répondre": "The model thinks before answering",
  "Le modèle répond": "The model answers",
  "Le modèle utilisé automatiquement par la synthèse vocale quand aucune voix n'est choisie dans le Studio. Le choix reste modifiable pour une génération ponctuelle.":
    "The model used automatically by speech synthesis when no voice is chosen in the Studio. The choice stays editable for a one-off generation.",
  "Le modèle utilisé par défaut pour la génération et retouche d'images (texte vers image, illustrations du chat, Studio).":
    "The default model for image generation and editing (text to image, chat illustrations, Studio).",
  "Le modèle vous demande quelque chose": "The model is asking you something",
  "Le mot de passe doit comporter au moins 8 caractères.":
    "The password must have at least 8 characters.",
  "Le mot de passe est enregistré dans ce lien — il ne sera ni affiché ni replacé ailleurs. Ne transmettez un tel lien qu'à vos propres machines.":
    "The password is stored in this link — it won't be shown or reused elsewhere. Only share such a link with your own machines.",
  "Le mot de passe est généré au premier démarrage de la passerelle.":
    "The password is generated on the gateway's first start.",
  "Le moteur choisi calcule les reponses. Le runtime integre charge du GGUF ; une extension peut en apporter un autre, avec ses propres formats de poids. Un seul est actif a la fois.":
    "The chosen engine computes the answers. The built-in runtime loads GGUF; an extension can bring another one, with its own weight formats. Only one is active at a time.",
  "Le moteur de conversation local ne charge pas ce dépôt. Gardé ici pour pouvoir libérer l'espace disque.":
    "The local chat engine does not load this repository. Kept here so you can free the disk space.",
  "Le moteur tourne avec": "The engine runs with",
  "Le moteur unique qui exécute vos modèles GGUF en local. Locaryn le télécharge et le maintient à jour (":
    "The single engine that runs your GGUF models locally. Locaryn downloads it and keeps it up to date (",
  "Le navigateur intégré s'ouvre dans l'application de bureau.":
    "The built-in browser opens in the desktop app.",
  "Le nom est celui que l'appareil se donne. Si ce n'est pas le vôtre, refusez.":
    "The name is the one the device gives itself. If it isn't yours, refuse.",
  "Le nom ne peut contenir que des lettres, des chiffres, « - » et « _ ».":
    "The name can only contain letters, digits, “-” and “_”.",
  "Le noyau décide de la mémoire, des skills et du fournisseur de cette conversation. Vous pouvez changer d'avis à chaque nouvelle conversation — le noyau Locaryn n'est jamais remplacé.":
    "The core decides this conversation's memory, skills and provider. You can change your mind with each new conversation — the Locaryn core is never replaced.",
  "Le noyau natif : vos providers Locaryn, vos règles, vos outils.":
    "The native core: your Locaryn providers, your rules, your tools.",
  "Le petit modèle relit les deux fils": "The small model rereads both threads",
  "Le plus puissant modèle Qwen 2.5 de 72B exécutable localement avec ~4 Go de VRAM.":
    "The most powerful 72B Qwen 2.5 model, runnable locally with ~4 GB of VRAM.",
  "Le plus sûr est de reprendre le même modèle que la dernière fois — ou un modèle dont le contexte est plus grand. Et si c'est le réglage de contexte qui a baissé, le remonter suffit : changer de modèle n'est pas obligatoire.":
    "The safest is to use the same model as last time — or a model with a larger context. And if the context setting went down, raising it is enough: changing models is not required.",
  "Le résultat d'un bloc de code exécuté : sortie ou page rendue.":
    "The result of a code block that ran: output or rendered page.",
  "Le script d'oblitération modifie directement les tenseurs d'activation du modèle pour supprimer le blocage des réponses. Cet outil est destiné **exclusivement à la recherche en cybersécurité, aux tests d'intrusion (pentesting encadré) et à l'audit de robustesse des LLM**.":
    "The obliteration script directly modifies the model's activation tensors to remove answer blocking. This tool is meant **exclusively for cybersecurity research, penetration testing (supervised pentesting) and LLM robustness audits**.",
  "Le serveur auquel ce poste est connecté utilise cette extension, et elle agit sur chaque machine : elle a besoin de tourner sur celle-ci pour s'en occuper.":
    "The server this machine is connected to uses this extension, and it acts on each machine: it needs to run on this one to take care of it.",
  "Le serveur n'indique pas d'où installer cette extension.":
    "The server does not say where to install this extension from.",
  "Le service n'a pas renvoyé d'image QR.": "The service did not return a QR image.",
  "Lecture d'images via projecteur mmproj": "Image reading through an mmproj projector",
  "Lecture de l'etat des moteurs…": "Reading engine status…",
  "Lecture de ses modèles…": "Reading its models…",
  "Lecture des archives…": "Reading archives…",
  "Lecture des modèles disponibles et de ce que votre PC peut faire tourner. Vos modèles déjà installés restent utilisables pendant ce temps.":
    "Reading the available models and what your PC can run. Your installed models stay usable meanwhile.",
  "Lecture des outils…": "Reading tools…",
  "Lecture du catalogue…": "Reading the catalog…",
  "Lecture et gestion d'agendas.": "Calendar reading and management.",
  "Lecture/écriture de fichiers avec contrôle d'accès configurable.":
    "File read/write with configurable access control.",
  Les: "The",
  "Les deux modèles à la fois :": "Both models at once:",
  "Les extensions (Image, Musique, Vidéo, 3D, Voix, etc.) contribuent dynamiquement leurs propres onglets et interfaces.":
    "Extensions (Image, Music, Video, 3D, Voice, etc.) dynamically add their own tabs and interfaces.",
  "Les extensions actives ajoutent leurs propres outils et interfaces multimodales.":
    "Active extensions add their own tools and multimodal interfaces.",
  "Les fichiers alternatifs ne seront pas ciblés automatiquement.":
    "Alternative files will not be targeted automatically.",
  "Les fonctionnalités spécialisées sont fournies par leurs extensions.":
    "Specialized features are provided by their extensions.",
  "Les marketplaces Claude Code sont des dépôts contenant":
    "Claude Code marketplaces are repositories containing",
  "Les modifications s'appliquent au prochain redémarrage du moteur (nouvelle session ou reload du modèle).":
    "Changes apply at the next engine restart (new session or model reload).",
  "Les moteurs d'exécution s'adaptent automatiquement aux plugins et extensions installés. Chaque modalité (image, vidéo, voix, musique, 3D, LLM) est portée par son moteur dédié.":
    "Execution engines adapt automatically to installed plugins and extensions. Each modality (image, video, voice, music, 3D, LLM) has its dedicated engine.",
  "Les mots de passe ne correspondent pas.": "Passwords do not match.",
  "Les notices complètes sont livrées dans": "Full notices are shipped in",
  "Les outils qu'elle a le droit d'appeler, séparés par des virgules (generate_image, generate_speech, read_file…). Vide : tout ce que l'application propose.":
    "The tools it may call, comma-separated (generate_image, generate_speech, read_file…). Empty: everything the app offers.",
  "Les pièces jointes partent à la fin de la réponse":
    "Attachments are sent at the end of the answer",
  "Les poids Safetensors existants ne seront ni relancés ni retéléchargés.":
    "Existing Safetensors weights will not be restarted or re-downloaded.",
  "Les poids de modèles, les moteurs et les fichiers temporaires vivent ici. Placez-le sur un disque avec de l'espace : une seule famille de modèles dépasse souvent 20 Go.":
    "Model weights, engines and temporary files live here. Put it on a disk with space: a single model family often exceeds 20 GB.",
  "Les sous-agents que le modèle lance, les plans et les téléchargements apparaissent ici pendant qu'ils tournent.":
    "Sub-agents the model launches, plans and downloads show here while they run.",
  "Les vieux échanges deviennent un résumé, les derniers restent":
    "Old exchanges become a summary, the latest stay",
  "Licences open-source": "Open-source licenses",
  "Lien copié": "Link copied",
  "Lier le bouton à une fonction": "Bind the button to a function",
  "Lire ce message en entier": "Read this message in full",
  "Lire, chercher et manipuler des dépôts Git.": "Read, search and manipulate Git repositories.",
  "Lire, extraire, créer, fusionner ou remplir des PDF.":
    "Read, extract, create, merge or fill PDFs.",
  "Listé pour la recherche : rien dans ce paquet ne peut tourner dans Locaryn.":
    "Listed for search: nothing in this package can run in Locaryn.",
  "Lit le code d'un script.": "Reads a script's code.",
  "Llama 3.1 70B customisé par NVIDIA pour des réponses plus utiles et performantes.":
    "Llama 3.1 70B customized by NVIDIA for more helpful and capable answers.",
  "Locaryn (cœur)": "Locaryn (core)",
  "Locaryn est bâti sur des moteurs sous licence permissive et embarque leurs notices :":
    "Locaryn is built on permissively licensed engines and ships their notices:",
  "Locaryn est à jour": "Locaryn is up to date",
  "Locaryn ne conserve encore aucune fiche sur vous.":
    "Locaryn doesn't keep any note about you yet.",
  "Locaryn peut lire, chercher, éditer et exécuter du code dans ce projet ou en chat libre.":
    "Locaryn can read, search, edit and run code in this project or in free chat.",
  "Longueur maximale d'une réponse": "Maximum answer length",
  "Lu pendant la tâche": "Read during the task",
  "Légèrement plus lourd que Q4_K_M pour un gain subtil.":
    "Slightly heavier than Q4_K_M for a subtle gain.",
  "Légères (Q2/Q3/IQ)": "Light (Q2/Q3/IQ)",
  "Mac Apple Silicon (32 Go unifiés)": "Apple Silicon Mac (32 GB unified)",
  "Maintenir la jauge pour compresser la conversation.":
    "Hold the gauge to compress the conversation.",
  "Maintenir pour compresser le contexte": "Hold to compress the context",
  "Marketplace ajoutée. Lancez « Actualiser » pour la lire.":
    "Marketplace added. Click “Refresh” to read it.",
  "Marketplace des morphs, skills et extensions": "Marketplace for morphs, skills and extensions",
  Masquer: "Hide",
  "Masquer le token": "Hide token",
  "Masquer les détails": "Hide details",
  "Masquer les détails ▲": "Hide details ▲",
  "Masquer les skills": "Hide skills",
  Maturité: "Maturity",
  "Maximum inconnu tant qu'aucun modèle n'est chargé : la place dépend du modèle qui sera chargé. Les jetons utilisés restent comptés, ~":
    "Maximum unknown while no model is loaded: the room depends on the model that will be loaded. Used tokens are still counted, ~",
  "Maximum — toutes les couches sur GPU": "Maximum — all layers on GPU",
  "Mes modèles installés": "My installed models",
  "Mettre à jour": "Update",
  "Mettre à jour le runtime": "Update the runtime",
  "Mise à jour": "Update",
  "Mise à jour dispo · v": "Update available · v",
  "Mise à jour disponible, espace disque bas dans le dossier des modèles.":
    "Update available, low disk space in the models folder.",
  "Mise à jour en cours…": "Updating…",
  "Mise à jour en lot annulée": "Batch update cancelled",
  "Mise à jour en lot terminée": "Batch update finished",
  "Mise à jour…": "Updating…",
  "Mises à jour": "Updates",
  "MoE 1T paramètres, architecture frontier pour orchestration multi-agents. Cloud uniquement via Ollama.":
    "1T-parameter MoE, frontier architecture for multi-agent orchestration. Cloud only via Ollama.",
  "MoE 2,8T paramètres, fenêtre de contexte 1M tokens, multimodal natif. Nécessite un compte Ollama cloud (Pro/Max).":
    "2.8T-parameter MoE, 1M-token context window, native multimodal. Needs an Ollama cloud account (Pro/Max).",
  "MoE 30B-A3B ultra-rapide avec contexte 198k pour raisonnement efficient.":
    "Ultra-fast 30B-A3B MoE with 198k context for efficient reasoning.",
  "MoE flagship 671B (37B actifs/token) pour le raisonnement général de pointe.":
    "671B flagship MoE (37B active/token) for cutting-edge general reasoning.",
  "MoE spécialisé pour le code et l'ingénierie logicielle (16B Lite & 236B Full).":
    "MoE specialized in code and software engineering (16B Lite & 236B Full).",
  "Mode aperçu navigateur : l'updater natif est désactivé.":
    "Browser preview mode: the native updater is disabled.",
  "Mode aperçu navigateur : la mise à jour automatique est désactivée.":
    "Browser preview mode: automatic update is disabled.",
  "Mode de connexion": "Connection mode",
  "Mode sans limite — responsabilité": "Unlimited mode — responsibility",
  "Modifications non enregistrées.": "Unsaved changes.",
  "Module complet avec intégration UI, slots déclaratifs et moteurs IA":
    "Full module with UI integration, declarative slots and AI engines",
  "Module sans interface personnalisée.": "Module without custom interface.",
  Modèle: "Model",
  "Modèle 12B développé avec NVIDIA pour contexte long.":
    "12B model developed with NVIDIA for long context.",
  "Modèle 22B spécialisé pour 80+ langages de programmation.":
    "22B model specialized in 80+ programming languages.",
  "Modèle 7B officiel iconique de Mistral AI.": "Mistral AI's iconic official 7B model.",
  "Modèle Local": "Local model",
  "Modèle Source à Oblitérer": "Source model to obliterate",
  "Modèle TTS par défaut": "Default TTS model",
  "Modèle TTS par défaut pour les notes vocales et le Studio audio.":
    "Default TTS model for voice notes and the audio Studio.",
  "Modèle agentic-coding MoE optimisé pour les workflows de développement (256k contexte natif).":
    "Agentic-coding MoE model optimized for development workflows (256k native context).",
  "Modèle compact et rapide 24B pour le code et le chat, ultra-fluide avec AirLLM sur GPU 2-4 Go.":
    "Compact and fast 24B model for code and chat, very smooth with AirLLM on a 2-4 GB GPU.",
  "Modèle conversationnel 9B chinois/anglais avec contexte 128k.":
    "9B Chinese/English chat model with 128k context.",
  "Modèle d'image par défaut": "Default image model",
  "Modèle d'un autre PC": "Model from another PC",
  "Modèle de base": "Base model",
  "Modèle de diffusion par défaut pour le Studio et les illustrations du chat.":
    "Default diffusion model for the Studio and chat illustrations.",
  "Modèle de raisonnement 70B exécutable sur petit GPU (4 Go VRAM) grâce au chargement couche par couche AirLLM.":
    "70B reasoning model runnable on a small GPU (4 GB VRAM) thanks to AirLLM layer-by-layer loading.",
  "Modèle de raisonnement profond spécialisé mathématiques, logique et résolution de problèmes complexes.":
    "Deep reasoning model specialized in maths, logic and complex problem solving.",
  "Modèle des petites tâches": "Small tasks model",
  "Modèle draft pour accélérer la génération": "Draft model to speed up generation",
  "Modèle local": "Local model",
  "Modèle local haute vitesse de Google pour la programmation, l'analyse multimodale et l'exécution d'outils (function calling).":
    "Google's high-speed local model for programming, multimodal analysis and tool execution (function calling).",
  "Modèle local installé à oblitérer (": "Installed local model to obliterate (",
  "Modèle multimodal 5.6B (texte + vision + audio/parole).":
    "5.6B multimodal model (text + vision + audio/speech).",
  "Modèle multimodal local optimisé pour l'exécution directe sur appareil (desktop/edge). Prise en charge du résumé, de la vision et du raisonnement rapide.":
    "Local multimodal model optimized for on-device execution (desktop/edge). Supports summarization, vision and fast reasoning.",
  "Modèle ouvert 14B de Microsoft aux performances de raisonnement exceptionnelles.":
    "Microsoft's open 14B model with exceptional reasoning performance.",
  "Modèle phare open-weight officiel 70B de Meta.":
    "Meta's official 70B open-weight flagship model.",
  "Modèle prêt pour le fine-tuning LoRA": "Model ready for LoRA fine-tuning",
  "Modèle trop lourd": "Model too heavy",
  "Modèle trop lourd pour être chargé": "Model too heavy to load",
  "Modèle vision-langage 3B dédié à l'OCR haute précision et token-efficient.":
    "3B vision-language model dedicated to high-precision, token-efficient OCR.",
  "Modèles (poids)": "Models (weights)",
  "Modèles en mémoire": "Models in memory",
  "Modèles généraux officiels de la suite Qwen 2.5 (0.5B à 72B).":
    "Official general models of the Qwen 2.5 suite (0.5B to 72B).",
  "Modèles légers ultra-performants de Google (variantes officielles 2B, 9B et 27B).":
    "Google's high-performance light models (official 2B, 9B and 27B variants).",
  "Modèles officiels Google spécialisés pour la complétion et la génération de code.":
    "Official Google models specialized in code completion and generation.",
  "Modèles officiels ultra-rapides et légers pour inférence locale.":
    "Official ultra-fast and light models for local inference.",
  "Modèles open-weight polyvalents de Meta (8B, 70B, 405B).":
    "Meta's versatile open-weight models (8B, 70B, 405B).",
  "Modèles raisonneurs officiels capables de réflexion logique étape par étape (1.5B à 671B).":
    "Official reasoning models capable of step-by-step logical thinking (1.5B to 671B).",
  "Mon compte": "My account",
  "Morph · ses connecteurs et skills, imposés au modèle":
    "Morph · its connectors and skills, forced on the model",
  "Morphs Locaryn, compétences et packs d'agents": "Locaryn Morphs, skills and agent packs",
  "Morphs Officiels Locaryn (Certifiés)": "Official Locaryn Morphs (Certified)",
  "Morphs du catalogue :": "Catalog Morphs:",
  "Morphs, Skills, MCP, configuration moteurs, thèmes et sécurité":
    "Morphs, Skills, MCP, engine setup, themes and security",
  "Morphs, skills et connecteurs": "Morphs, skills and connectors",
  "Mot de passe (8 caractères minimum)": "Password (8 characters minimum)",
  "Mot de passe du tableau de bord": "Dashboard password",
  "Mot de passe sur l'autre PC": "Password on the other PC",
  "Moteur AirLLM : exécuter localement des modèles trop lourds pour ce PC":
    "AirLLM engine: run models too heavy for this PC locally",
  "Moteur d'IA local unifié — chat, agents, vision, RAG et génération d'images, sans dépendre du cloud.":
    "Unified local AI engine — chat, agents, vision, RAG and image generation, without relying on the cloud.",
  "Moteur d'Inférence": "Inference Engine",
  "Moteur d'apprentissage LoRA local et manipulation de représentations (RepE) pour décensurer ou orienter les poids.":
    "Local LoRA training engine and representation engineering (RepE) to uncensor or steer weights.",
  "Moteur d'inférence open-source : les gros modèles tournent sur un GPU 4 Go de VRAM en chargeant les couches une par une.":
    "Open-source inference engine: big models run on a 4 GB VRAM GPU by loading layers one at a time.",
  "Moteur de composition musicale et génération audio par diffusion sonore temporelle.":
    "Music composition and audio generation engine using temporal audio diffusion.",
  "Moteur de diffusion locale pour modèles SD 1.5, SDXL, Flux et retouche inpaint/outpaint sans dépendance cloud.":
    "Local diffusion engine for SD 1.5, SDXL, Flux models and inpaint/outpaint editing with no cloud dependency.",
  "Moteur de diffusion spatio-temporelle pour la génération et l'animation de vidéos locales.":
    "Spatio-temporal diffusion engine for local video generation and animation.",
  "Moteur de reconstruction et génération de maillages 3D et textures volumétriques.":
    "3D mesh and volumetric texture reconstruction and generation engine.",
  "Moteur de streaming couche par couche permettant d'exécuter des modèles géants sur GPU modeste (4 Go VRAM).":
    "Layer-by-layer streaming engine to run giant models on a modest GPU (4 GB VRAM).",
  "Moteur de synthèse vocale temps réel haute fidélité (80M/100M) et clonage vocal hors-ligne.":
    "High-fidelity real-time speech synthesis engine (80M/100M) and offline voice cloning.",
  "Moteur principal d'inférence GGUF local pour le chat, la programmation assistée, l'agentique et les modèles multimodaux.":
    "Main local GGUF inference engine for chat, assisted coding, agents and multimodal models.",
  "Moteur, projets, extensions, apparence, stockage…":
    "Engine, projects, extensions, appearance, storage…",
  "Moteurs de conversation": "Chat engines",
  "Moteurs de conversation (": "Chat engines (",
  "Mots déclenchant une escalade": "Words that trigger an escalation",
  "Multimodal officiel : analyse conjointe d'images et de textes.":
    "Official multimodal: joint analysis of images and texts.",
  "MÉMOIRE DU COMPTE": "ACCOUNT MEMORY",
  Mémoire: "Memory",
  "Mémoire RAM Système": "System RAM",
  "Mémoire modifiée par instruction": "Memory changed by instruction",
  "Mémoire persistante sous forme de graphe de connaissances.":
    "Persistent memory as a knowledge graph.",
  "Mémoire vidée": "Memory cleared",
  "Mémoriser le mot de passe (trousseau du système)": "Remember the password (system keychain)",
  "Méthode d'Ablation": "Ablation method",
  "Méthode d'Ablation Vectorielle": "Vector ablation method",
  "Ne pas ranger": "Don't file",
  "Ne refuse jamais. Prévient, puis charge — au prix d'un ralentissement sévère, voire d'un plantage.":
    "Never refuses. Warns, then loads — at the cost of a severe slowdown, or even a crash.",
  "Ne rien poser": "Set nothing",
  "Nettoyer les fichiers temporaires": "Clean up temporary files",
  "Neutralisez les filtres de refus de n'importe quel modèle local via représentation vectorielle (Open Source Script).":
    "Neutralize the refusal filters of any local model through vector representation (Open Source Script).",
  "Niveau de Confiance Global du Projet": "Project-wide trust level",
  "Niveau de confiance du projet —": "Project trust level —",
  "Nom (seulement si le JSON n'en contient pas)": "Name (only if the JSON has none)",
  "Nom affiché / Alias": "Display name / Alias",
  "Nom de la source": "Source name",
  "Nom de la source (ex: notes-archi.md)": "Source name (e.g. notes-archi.md)",
  "Nom du connecteur MCP": "MCP connector name",
  "Nom du lot (ex: Indexation de 80 fichiers / Audit de sécurité)...":
    "Batch name (e.g. Indexing 80 files / Security audit)...",
  "Nom du modèle, identifiant HuggingFace ou URL :": "Model name, HuggingFace ID or URL:",
  "Nom du serveur": "Server name",
  "Nommage, rangement et résumés courts sans interrompre le modèle principal.":
    "Naming, filing and short summaries without interrupting the main model.",
  "Non Recommandé": "Not recommended",
  "Non couvert par ce moteur (nécessiterait une pile séparée) :":
    "Not covered by this engine (would need a separate stack):",
  "Non exécutable": "Not runnable",
  "Non installé — requis pour discuter avec un modèle local":
    "Not installed — required to chat with a local model",
  "Non revendiqué": "Unclaimed",
  "Nouveau dossier enregistré. Les anciens fichiers sont restés sur place — les modèles déjà téléchargés n'apparaîtront plus tant qu'ils ne sont pas déplacés.":
    "New folder saved. Old files stayed in place — already downloaded models won't show until they are moved.",
  "Nouvelle conversation": "New conversation",
  "Nouvelle figure": "New figure",
  "Noyau Locaryn (défaut)": "Locaryn core (default)",
  "Noyau alternatif installé.": "Alternative core installed.",
  "Noyau de la conversation": "Conversation core",
  "Objectif à atteindre": "Goal to reach",
  "Travail en équipe": "Team work",
  "Plusieurs sous-agents en parallèle, chacun sur le modèle qui convient ; la conversation relit et assemble":
    "Several sub-agents in parallel, each on the right model; the conversation reviews and assembles",
  "Oblitération des poids en cours...": "Obliterating weights...",
  "Oblitération en cours...": "Obliteration in progress...",
  "Oblitération en cours…": "Obliteration in progress…",
  "Options de la note vocale": "Voice note options",
  "Orchestration imposée, même si le modèle la juge inutile":
    "Forced orchestration, even if the model deems it useless",
  "Ou coller du texte": "Or paste text",
  "Outil en cours": "Tool running",
  Outils: "Tools",
  "Outils Flutter et Dart pour l'agent.": "Flutter and Dart tools for the agent.",
  "Outils de cette machine": "This machine's tools",
  "Outils de «": "Tools of “",
  "Ouvrez une conversation pour voir ses modifications.": "Open a conversation to see its changes.",
  "Ouvrez une conversation pour voir son dossier.": "Open a conversation to see its folder.",
  "Ouvrir dans l'espace de travail": "Open in the workspace",
  "Ouvrir dans le navigateur intégré": "Open in the built-in browser",
  "Ouvrir l'emplacement": "Open location",
  "Ouvrir l'espace de travail : navigateur, fichiers, terminal, modifications":
    "Open the workspace: browser, files, terminal, changes",
  "Ouvrir la version GGUF compatible": "Open the compatible GGUF version",
  "Ouvrir le dossier": "Open folder",
  "Ouvrir le dossier des modèles": "Open the models folder",
  "Ouvrir le résultat": "Open the result",
  "Ouvrir le studio d'oblitération de modèle dans le menu Entraînement":
    "Open the model obliteration studio in the Training menu",
  "Ouvrir le tableau de bord": "Open the dashboard",
  "Ouvrir le tableau de bord dans le navigateur": "Open the dashboard in the browser",
  "Ouvrir le terminal et les journaux": "Open the terminal and logs",
  "Ouvrir les paramètres de génération du modèle actif":
    "Open the active model's generation settings",
  "Ouvrir les réglages": "Open settings",
  "Ouvrir les réglages de l'application": "Open the app settings",
  "Ouvrir les réglages de performance et de permissions":
    "Open performance and permission settings",
  "Ouvrir un dossier local": "Open a local folder",
  "Ouvrir une conversation": "Open a conversation",
  "PRÉFÉRENCES DU COMPTE": "ACCOUNT PREFERENCES",
  "Pack de compétences agentiques et consignes procédurales":
    "Pack of agent skills and procedural instructions",
  "Page précédente": "Previous page",
  "Page web": "Web page",
  "PagedAttention / vLLM (serveur GPU séparé)": "PagedAttention / vLLM (separate GPU server)",
  "Paire de clés": "Key pair",
  "Paquets installés (": "Installed packages (",
  "Par défaut l'application ne pose rien devant le modèle : il répond avec son propre caractère, exactement comme lancé hors d'elle. Écrivez ici ce que vous voulez qu'il soit — un compagnon, un correcteur, un spécialiste d'un domaine — et ce texte ouvrira chaque conversation.":
    "By default the app sets nothing before the model: it answers with its own personality, exactly as if run outside it. Write here what you want it to be — a companion, a proofreader, a domain specialist — and this text will open every conversation.",
  Paramètres: "Settings",
  "Paramètres & Profil": "Settings & Profile",
  "Paramètres Système & Profil": "System Settings & Profile",
  "Paramètres de génération du modèle": "Model generation settings",
  "Paramètres de l'Oblitération (RepE Refusal Ablation)":
    "Obliteration settings (RepE Refusal Ablation)",
  "Paramètres du Modèle": "Model Settings",
  "Paramètres du Projet :": "Project Settings:",
  "Paramètres du chat": "Chat Settings",
  "Paramètres du projet": "Project settings",
  Parcourir: "Browse",
  "Parcourir le catalogue": "Browse the catalog",
  "Parcourir le marketplace": "Browse the marketplace",
  "Pas de LaTeX sur ce poste.": "No LaTeX on this machine.",
  "Pas de clé": "No key",
  "Pas de description fournie.": "No description provided.",
  "Passer cette conversation en Autonome": "Switch this conversation to Autonomous",
  "Passerelle de démonstration": "Demo gateway",
  "Passerelle technique d'outils pour les modèles de langage":
    "Technical tool gateway for language models",
  "Performance et permissions": "Performance and permissions",
  "Permet d'inspecter les tables et schémas SQL pour ce projet.":
    "Lets you inspect SQL tables and schemas for this project.",
  "Permet de tester de grands modèles sur très petit matériel.":
    "Lets you test big models on very small hardware.",
  "Permissions de la conversation": "Conversation permissions",
  "Permissions demandées :": "Requested permissions:",
  "Permissions des nouvelles conversations": "Permissions for new conversations",
  Personnalisé: "Custom",
  "Personnalité et ton": "Personality and tone",
  "Petites tâches": "Small tasks",
  "Petites tâches, voix et images": "Small tasks, voice and images",
  "Pièces jointes des chats": "Chat attachments",
  "Plan par étapes": "Step-by-step plan",
  "Plein écran": "Full screen",
  "Plugin de démonstration — aperçu avant installation.":
    "Demo plugin — preview before installing.",
  "Plus récents": "Newest",
  Portée: "Scope",
  "Posez votre question à Locaryn…": "Ask Locaryn…",
  "Posée par": "Asked by",
  "Pour PC modestes ou cartes 4-8 Go.": "For modest PCs or 4-8 GB cards.",
  "Pour PC portables ou configurations 8 Go.": "For laptops or 8 GB setups.",
  "Pour pouvoir appliquer le script d'oblitération, vous devez au préalable **télécharger et installer un modèle local** (depuis le Marketplace de Modèles).":
    "To apply the obliteration script, you must first **download and install a local model** (from the Model Marketplace).",
  "Pour un téléphone sur le même Wi‑Fi. Le QR porte l'adresse locale et l'empreinte du certificat ; mDNS évite la saisie.":
    "For a phone on the same Wi‑Fi. The QR carries the local address and the certificate fingerprint; mDNS avoids typing.",
  "Pour éviter toute configuration à vos collègues, faites-leur scanner le QR du panneau d'appairage : il porte l'adresse et cette empreinte. Il n'y a aucun code à saisir.":
    "To spare your colleagues any setup, have them scan the QR of the pairing panel: it carries the address and this fingerprint. There is no code to type.",
  "Première phrase": "First sentence",
  "Presse-papiers illisible (permission refusée) — collez le lien à la main (Ctrl+V).":
    "Clipboard unreadable (permission denied) — paste the link by hand (Ctrl+V).",
  "Presse-papiers inaccessible — collez le lien à la main (Ctrl+V).":
    "Clipboard inaccessible — paste the link by hand (Ctrl+V).",
  "Privilégiez Q6_K face à Q8_0 pour économiser 20% d'espace sans perte perceptible.":
    "Prefer Q6_K over Q8_0 to save 20% of space with no noticeable loss.",
  "Profil de base": "Base profile",
  "Profil en cours": "Current profile",
  "Profil enregistré": "Profile saved",
  "Profil local": "Local profile",
  "Profil local, identité, préférences et mémoire":
    "Local profile, identity, preferences and memory",
  "Profil personnalisé — les réglages ci-dessous s'appliquent directement":
    "Custom profile — the settings below apply directly",
  Projets: "Projects",
  "Projets de code personnels et scolaires.": "Personal and school code projects.",
  "Proposée à l'ouverture d'une conversation. Laissez vide pour commencer sur une page blanche.":
    "Offered when opening a conversation. Leave empty to start on a blank page.",
  "Prudence au chargement d'un modèle": "Caution when loading a model",
  "Préférences des modèles": "Model preferences",
  "Préparation de l'écoute…": "Preparing playback…",
  "Préparation du catalogue…": "Preparing the catalog…",
  "Préparation du code…": "Preparing the code…",
  "Préparation…": "Preparing…",
  "Prépare un fichier de requêtes au format JSONL pour un traitement par lots. La soumission réelle vers un fournisseur (OpenAI, Anthropic, DeepSeek) ou vers le moteur local n'est pas encore implémentée.":
    "Prepares a JSONL request file for batch processing. Real submission to a provider (OpenAI, Anthropic, DeepSeek) or to the local engine is not implemented yet.",
  "Préparer un lot de traitement (JSONL)": "Prepare a processing batch (JSONL)",
  "Prête à être écoutée": "Ready to play",
  "Publication du point d'accès": "Publishing the access point",
  "Python 3.12 et Node 22 sont installés ici.": "Python 3.12 and Node 22 are installed here.",
  "Python introuvable — installez Python 3.10+ (Paramètres → Système).":
    "Python not found — install Python 3.10+ (Settings → System).",
  "Q3_K / IQ (Ultra léger)": "Q3_K / IQ (Ultra light)",
  "Q4_K_M (Recommandé)": "Q4_K_M (Recommended)",
  "Q6_K / Q8_0 (Qualité max)": "Q6_K / Q8_0 (Max quality)",
  "QR de connexion": "Connection QR",
  "Quand Locaryn vous prévient": "When Locaryn notifies you",
  "Quand prévenir": "When to notify",
  "Quelque chose est cassé": "Something is broken",
  "Quelque chose est cassé dans cette conversation": "Something is broken in this conversation",
  "Qui doit le voir": "Who should see it",
  "Qui la voit": "Who sees it",
  "Quitter Locaryn": "Quit Locaryn",
  "Quitter la sélection": "Leave selection",
  "Quitter le mode éphémère et ouvrir une conversation normale":
    "Leave ephemeral mode and open a normal conversation",
  "Rafraîchir le catalogue": "Refresh the catalog",
  "Ranger cette conversation dans": "File this conversation in",
  "Ranger dans": "File in",
  "Ranger dans ▾": "File in ▾",
  "Rangez d'un geste": "File in one move",
  "Recalculer le diagnostic": "Recompute the diagnosis",
  "Recevoir les pré-versions expérimentales de l'application Locaryn pour tester les nouvelles fonctionnalités avant leur sortie officielle.":
    "Receive experimental pre-releases of the Locaryn app to test new features before their official release.",
  "Recharger le modèle maintenant": "Reload the model now",
  "Recherche Web Brave & Scraper": "Brave Web Search & Scraper",
  "Recherche d'images": "Image search",
  "Recherche en cours…": "Searching…",
  "Recherche sur le web et extraction de contenu.": "Web search and content extraction.",
  "Recherche web": "Web search",
  "Recherche web en cours…": "Web search in progress…",
  "Recherche web terminée": "Web search finished",
  "Recherche web terminée (1 échec)": "Web search finished (1 failure)",
  "Rechercher dans les archives": "Search the archives",
  "Rechercher parmi les modèles (Gemma 4, Kimi K3, MiMo, GLM 5.2, DeepSeek-R1, Qwen2.5...)":
    "Search models (Gemma 4, Kimi K3, MiMo, GLM 5.2, DeepSeek-R1, Qwen2.5...)",
  "Rechercher parmi vos modèles installés…": "Search your installed models…",
  "Rechercher une conversation ou un projet…": "Search a conversation or a project…",
  "Rechercher une conversation par titre ou projet…": "Search a conversation by title or project…",
  "Rechercher une extension…": "Search an extension…",
  "Recommandées (": "Recommended (",
  "Recommandés pour mon PC": "Recommended for my PC",
  "Refusez ce qui n'est pas nécessaire : l'extension se charge quand même, sans la fonctionnalité concernée.":
    "Refuse what isn't needed: the extension still loads, without the feature concerned.",
  "Rejeter cet appareil": "Reject this device",
  "Rejoue la demande jusqu'à ce qu'elle se vérifie (2 à 5 fois)":
    "Replays the request until it checks out (2 to 5 times)",
  "Relais actif": "Relay active",
  "Relancé après échec de la vérification": "Restarted after the check failed",
  "Reliez ce profil à un serveur privé uniquement si vous souhaitez synchroniser ou exécuter des tâches à distance. Sinon, le mode local reste actif.":
    "Link this profile to a private server only if you want to sync or run tasks remotely. Otherwise, local mode stays active.",
  "Relire la liste chez le fournisseur": "Reload the list from the provider",
  "Relire le diff": "Review the diff",
  "Relire les changements du projet : bugs, sécurité, simplifications":
    "Review the project's changes: bugs, security, simplifications",
  "Relisez ce qui sera lancé sur cet ordinateur : rien ne tourne avant votre accord.":
    "Review what will run on this computer: nothing runs before you agree.",
  "Relit un diff et signale ce qui casse.": "Reviews a diff and reports what breaks.",
  "Remettre la conversation dans sa liste": "Put the conversation back in its list",
  "Remis — lu à la prochaine étape": "Delivered — read at the next step",
  Renommer: "Rename",
  "Renseignez votre identifiant et votre mot de passe.": "Enter your login and password.",
  "Repartir de zéro": "Start over",
  "Repli gratuit tant que le quota tient.": "Free fallback while the quota lasts.",
  "Replier ou déplier le panneau latéral": "Collapse or expand the side panel",
  Reprendre: "Resume",
  "Reprendre l'envoi": "Resume sending",
  "Requis pour télécharger les dépôts restreints (": "Required to download gated repositories (",
  "Requis seulement si votre administrateur vous en a fourni un. Choisissez le fichier « .pem » qu'il vous a transmis.":
    "Only required if your administrator gave you one. Choose the “.pem” file they sent you.",
  "Requêtes en parallèle": "Parallel requests",
  "Requêtes simultanées (utile pour plusieurs agents)":
    "Simultaneous requests (useful for several agents)",
  Retirer: "Remove",
  "Retirer ce détail": "Remove this detail",
  "Retirer cet emplacement": "Remove this location",
  "Retirer de la file": "Remove from queue",
  "Retirer l'avatar": "Remove avatar",
  "Retirer l'image": "Remove image",
  "Retirer le document": "Remove document",
  "Retirer le fichier": "Remove file",
  Retour: "Back",
  "Retour au dossier": "Back to folder",
  "Retrouvez et reprenez l'ensemble de vos échanges récents par espace de travail. Cliquez sur une conversation pour l'ouvrir directement dans le Chat. Les conversations archivées sont rangées dans":
    "Find and resume all your recent exchanges by workspace. Click a conversation to open it directly in the Chat. Archived conversations are filed in",
  "Revenir au mode local": "Back to local mode",
  "Revenir au modèle de ce PC": "Back to this PC's model",
  "Revenir aux modèles installés": "Back to installed models",
  "Revenir aux réglages recommandés": "Back to recommended settings",
  "Revoir la présentation": "Replay the introduction",
  "Rien n'est chargé ni déchargé : la petite tâche passe par le modèle déjà en mémoire. C'est le choix le plus rapide, et le seul qui ne prenne pas de VRAM.":
    "Nothing is loaded or unloaded: the small task goes through the model already in memory. It's the fastest choice, and the only one that takes no VRAM.",
  "Rien n'est perdu ici. Une conversation en ressort telle qu'elle était ; la supprimer est un geste séparé, et définitif.":
    "Nothing is lost here. A conversation comes back out as it was; deleting it is a separate, permanent action.",
  "Rien n'est posé. Quand des outils sont offerts, l'assistant reçoit seulement de quoi s'en servir correctement : aller lire un fichier plutôt qu'en deviner le contenu.":
    "Nothing is set. When tools are offered, the assistant only receives what it needs to use them properly: read a file rather than guess its content.",
  "Rien pour l'instant.": "Nothing yet.",
  "Rien à afficher": "Nothing to show",
  "Rien à compresser pour l'instant": "Nothing to compress yet",
  "Rien à compresser pour l'instant.": "Nothing to compress yet.",
  "Rien à saisir sur le téléphone : le code porte l'adresse et l'empreinte du certificat.":
    "Nothing to type on the phone: the code carries the address and the certificate fingerprint.",
  Risqué: "Risky",
  "Routé vers Anthropic, avec repli automatique.": "Routed to Anthropic, with automatic fallback.",
  "Routé vers OpenAI.": "Routed to OpenAI.",
  "Runtime installé": "Runtime installed",
  "Runtime llama.cpp, capacités, adaptateurs LoRA":
    "llama.cpp runtime, capabilities, LoRA adapters",
  "Rust + Tauri v2 + React (cœur in-process)": "Rust + Tauri v2 + React (in-process core)",
  "Récupère une page web et la convertit pour le modèle.":
    "Fetches a web page and converts it for the model.",
  "Réduire la file": "Collapse the queue",
  "Réentraînable / LoRA": "Trainable / LoRA",
  Réflexion: "Reasoning",
  "Réflexion courte": "Short reasoning",
  "Réflexion désactivée": "Reasoning off",
  "Réflexion illimitée, très lent": "Unlimited reasoning, very slow",
  "Réflexion élevée": "High reasoning",
  "Réflexion…": "Thinking…",
  "Réfléchit longuement": "Thinks at length",
  "Réfléchit…": "Thinking…",
  Réglages: "Settings",
  "Réglages avancés": "Advanced settings",
  Régler: "Configure",
  Réinitialiser: "Reset",
  "Réinitialiser l'apparence": "Reset appearance",
  "Réinitialiser les paramètres": "Reset settings",
  "Réinstalle depuis la même source (github:…, dossier ou zip)":
    "Reinstalls from the same source (github:…, folder or zip)",
  "Réinstalle depuis sa source chaque extension non à jour — installée ou vue dans le catalogue.":
    "Reinstalls from its source each out-of-date extension — installed or seen in the catalog.",
  "Répartit les couches du modèle sur plusieurs machines exécutant":
    "Spreads the model's layers across several machines running",
  "Répartit les couches sur plusieurs machines (RPC)":
    "Spreads layers across several machines (RPC)",
  "Répertoire distant cible": "Target remote directory",
  Répondre: "Reply",
  "Réponse JSON": "JSON answer",
  "Réponse directe, le plus rapide": "Direct answer, fastest",
  "Réponses directes, plus rapides": "Direct answers, faster",
  "Répéter jusqu'à ce que ça passe": "Repeat until it passes",
  "Réseau local": "Local network",
  "Résolution de problèmes par séquences de raisonnement.":
    "Problem solving through reasoning sequences.",
  Résumé: "Summary",
  "Rétablir le réglage par défaut": "Restore the default setting",
  "Rétablir par défaut (Éthique)": "Restore default (Ethical)",
  "S'installe et fonctionne tel quel.": "Installs and works as is.",
  "SLM 3.8B edge-ready avec function calling natif, raisonnement et multilingue.":
    "Edge-ready 3.8B SLM with native function calling, reasoning and multilingual.",
  "Saisissez le code à 6 chiffres affiché sous le QR de l'hôte.":
    "Enter the 6-digit code shown under the host's QR.",
  "Sans cette bannière, un outil peut attendre votre accord sans que rien ne le signale.":
    "Without this banner, a tool may wait for your approval with nothing signalling it.",
  "Sans identifiant ni mot de passe": "Without login or password",
  "Sans limite": "No limit",
  "Sans titre": "Untitled",
  Sauvegardé: "Saved",
  "Scannez avec le téléphone": "Scan with the phone",
  "Se connecter": "Sign in",
  "Se connecter au serveur": "Connect to the server",
  "Se connecter à ce serveur ?": "Connect to this server?",
  "Se connecter à un serveur": "Connect to a server",
  "Sections du compte": "Account sections",
  Serveur: "Server",
  "Serveur & fonctions": "Server & features",
  "Serveur SSH": "SSH server",
  "Serveur actif": "Server running",
  "Serveur arrêté": "Server stopped",
  "Serveur de référence exposant prompts, ressources et outils.":
    "Reference server exposing prompts, resources and tools.",
  "Serveur distant": "Remote server",
  "Serveur distant (Mode Client)": "Remote server (Client mode)",
  "Serveurs déjà rencontrés": "Servers already met",
  "Service & Mode Serveur": "Service & Server Mode",
  "Service Locaryn, accès local et appairage": "Locaryn service, local access and pairing",
  "Ses conversations": "Its conversations",
  "Seulement le calcul du modèle": "Only the model computation",
  "Seulement quand la fenêtre n'est pas au premier plan": "Only when the window is not in front",
  "Seuls les messages sont estimés : la mesure complète arrive au prochain message.":
    "Only messages are estimated: the full measurement comes with the next message.",
  "Signature du certificat": "Certificate signature",
  "Skills, connecteurs et extensions": "Skills, connectors and extensions",
  Sombre: "Dark",
  "Sombre par défaut. En clair, l'accent s'assombrit tout seul pour rester lisible.":
    "Dark by default. In light mode, the accent darkens by itself to stay readable.",
  "Soumettre le lot": "Submit the batch",
  "Sous-agent en échec": "Sub-agent failed",
  "Spécialisation locale de Google pour la programmation, l'analyse syntaxique et le refactoring d'architectures complètes.":
    "Google's local specialization for programming, parsing and refactoring whole architectures.",
  "Spécialiste mondial officiel du code et du refactoring.":
    "Official world-class code and refactoring specialist.",
  Spéculatif: "Speculative",
  "Standard universel pour 90% des usages.": "Universal standard for 90% of uses.",
  "Station / Serveur (64 Go RAM, 24 Go VRAM)": "Workstation / Server (64 GB RAM, 24 GB VRAM)",
  Stockage: "Storage",
  Stocké: "Stored",
  "Stocké uniquement sur cette machine (localStorage de l'application) : le token persiste entre les ouvertures, n'est écrit dans aucun fichier du projet et ne peut donc jamais être poussé sur GitHub. Une réinstallation depuis le dépôt ne le fera pas réapparaître.":
    "Stored only on this machine (the app's localStorage): the token persists between launches, is written to no project file and so can never be pushed to GitHub. A reinstall from the repository won't bring it back.",
  "Studio d'Oblitération RepE": "RepE Obliteration Studio",
  "Studio d'Oblitération de Modèle (RepE Ablation)": "Model Obliteration Studio (RepE Ablation)",
  "Studio d'Oblitération de Modèle (RepE Refusal Ablation)":
    "Model Obliteration Studio (RepE Refusal Ablation)",
  "Studio de génération": "Generation studio",
  "Suivre la réponse": "Follow the answer",
  "Supprime un objet et ses enfants.": "Deletes an object and its children.",
  Supprimer: "Delete",
  "Supprimer ce modèle du disque dur local": "Delete this model from the local disk",
  "Supprimer définitivement": "Delete permanently",
  "Supprimer définitivement ?": "Delete permanently?",
  "Sur Linux, l'application ne se met pas à jour toute seule : téléchargez la dernière version depuis GitHub.":
    "On Linux, the app doesn't update by itself: download the latest version from GitHub.",
  "Sur ce PC": "On this PC",
  "Sur l'autre PC": "On the other PC",
  "Sur l'autre PC : activez le mode serveur et créez un identifiant (Réglages → Serveur & fonctions). Son adresse s'y affiche.":
    "On the other PC: enable server mode and create a login (Settings → Server & features). Its address shows there.",
  "Sur le téléphone, ouvrez Locaryn et touchez « Scanner un QR code » : la session de":
    "On the phone, open Locaryn and tap “Scan a QR code”: the session of",
  "Sur le téléphone, ouvrez Locaryn et touchez « Scanner un QR code ». L'appareil photo du téléphone seul ne sait pas lire ce code.":
    "On the phone, open Locaryn and tap “Scan a QR code”. The phone's camera app alone can't read this code.",
  "Sur quoi repose cette estimation": "What this estimate is based on",
  "Sur tous mes appareils.": "On all my devices.",
  "Synthèse vocale": "Speech synthesis",
  Système: "System",
  Sécurité: "Safe",
  Sélectionner: "Select",
  "Sélectionner cet emplacement": "Select this location",
  "Sélectionner plusieurs conversations": "Select several conversations",
  "Sélectionner votre profil matériel :": "Select your hardware profile:",
  "Sélectionnez ou créez une conversation": "Select or create a conversation",
  'Sélectionnez un modèle et cliquez sur "Lancer l\'Oblitération".':
    'Select a model and click "Start obliteration".',
  "Sélectionnez un modèle installé et validez la décharge pour démarrer...":
    "Select an installed model and accept the waiver to start...",
  "Sélection…": "Selection…",
  "Taille jusqu'à": "Size up to",
  "Taille supérieure à": "Size above",
  "Tapez une adresse ou une recherche. Le modèle peut aussi ouvrir, lire et remplir des pages ici.":
    "Type an address or a search. The model can also open, read and fill pages here.",
  "Terminal / Journaux": "Terminal / Logs",
  Terminer: "Finish",
  Terminé: "Done",
  "Terminées ·": "Finished ·",
  "Tester comme un humain": "Test like a human",
  "Tester la recherche": "Test the search",
  "Tester les composants de votre PC et analyser les performances d'inférence":
    "Test your PC's hardware and analyze inference performance",
  Thème: "Theme",
  "Tient dans votre VRAM — fluide sur GPU": "Fits in your VRAM — smooth on GPU",
  "Token d'accès HuggingFace": "HuggingFace access token",
  "Token pour les dépôts restreints (modèles gated)": "Token for gated repositories (gated models)",
  "Toujours demander la confirmation de l'utilisateur avant d'exécuter une commande terminal":
    "Always ask the user before running a terminal command",
  "Toujours demander la confirmation de l'utilisateur avant d'écrire ou modifier un fichier":
    "Always ask the user before writing or modifying a file",
  Tous: "All",
  "Tous (": "All (",
  "Tous les experts sur le CPU (-cmoe)": "All experts on the CPU (-cmoe)",
  "Tous les paramètres →": "All settings →",
  "Tous les réglages": "All settings",
  "Tous les shards de cette variante seront supprimés définitivement.":
    "All shards of this variant will be permanently deleted.",
  "Tout GPU": "All GPU",
  "Tout activer": "Enable all",
  "Tout ce qui est stocké localement : les modèles de conversation et ceux qu'une extension utilise.":
    "Everything stored locally: chat models and those an extension uses.",
  "Tout désactiver": "Disable all",
  "Tout effacer": "Erase all",
  "Tout le GPU": "Whole GPU",
  "Tout le projet": "Whole project",
  "Tout oublier": "Forget everything",
  "Tout passe alors par l'autre machine : conversations, projets, fichiers, et les outils (terminal, MCP). Pour n'emprunter que son modèle en gardant les outils ici, utilisez « Modèle d'un autre PC » ci-dessus.":
    "Everything then goes through the other machine: conversations, projects, files, and tools (terminal, MCP). To borrow only its model while keeping the tools here, use “Model from another PC” above.",
  "Tout reste en local (aucun envoi vers Internet). La première indexation démarre un petit serveur d'embeddings : comptez quelques secondes.":
    "Everything stays local (nothing sent to the Internet). The first indexing starts a small embeddings server: allow a few seconds.",
  "Tout sélectionner": "Select all",
  "Tout à gauche, Auto : le moteur prend la mémoire vidéo que le modèle laisse libre.":
    "Far left, Auto: the engine takes the video memory the model leaves free.",
  "Tout → CPU": "All → CPU",
  Toutes: "All",
  "Toutes (": "All (",
  "Toutes les années": "All years",
  "Toutes les couches sur le GPU quand elles tiennent": "All layers on the GPU when they fit",
  "Toutes les marques": "All brands",
  "Toutes les variantes…": "All variants…",
  "Toutes tailles": "All sizes",
  "Traitement par lots asynchrone à moitié prix": "Asynchronous batch processing at half price",
  "Transmises au processus du serveur — c'est ici qu'une clé d'API se passe, pas dans la commande.":
    "Passed to the server process — this is where an API key goes, not in the command.",
  "Trier les modèles": "Sort models",
  "Trop gros pour la VRAM, mais tourne via la RAM (offload CPU, plus lent)":
    "Too big for VRAM, but runs through RAM (CPU offload, slower)",
  "Trop lourd pour la VRAM/RAM de ce PC, mais exécutable localement via AirLLM":
    "Too heavy for this PC's VRAM/RAM, but runnable locally through AirLLM",
  "Très faible mémoire mais perte de précision possible.":
    "Very low memory but possible precision loss.",
  "Très longue fenêtre de contexte.": "Very long context window.",
  "Très économe en RAM.": "Very light on RAM.",
  "Tu es… (laissez vide pour ne rien poser devant le modèle)":
    "You are… (leave empty to set nothing before the model)",
  "Tunnel actif": "Tunnel active",
  "Tunnel arrêté": "Tunnel stopped",
  "TurboQuant KV 3-bit (papier Google réel, mais non mergé dans llama.cpp)":
    "TurboQuant 3-bit KV (real Google paper, but not merged in llama.cpp)",
  Tâches: "Tasks",
  "Tâches longues": "Long tasks",
  Téléchargement: "Download",
  "Téléchargement du runtime…": "Downloading the runtime…",
  "Téléchargement en cours…": "Downloading…",
  Téléchargements: "Downloads",
  Télécharger: "Download",
  "Télécharger ce modèle": "Download this model",
  "Télécharger un modèle": "Download a model",
  "Téléchargez n'importe quel modèle depuis HuggingFace, Ollama ou une URL directe.":
    "Download any model from HuggingFace, Ollama or a direct URL.",
  "URL du serveur Locaryn": "Locaryn server URL",
  "Un QR à scanner depuis l'app mobile : la session de ce compte s'ouvre sans mot de passe":
    "A QR to scan from the mobile app: this account's session opens without a password",
  "Un appareil": "A device",
  "Un appareil tente de s'appairer, ou vient de se connecter au mode serveur.":
    "A device is trying to pair, or has just connected to server mode.",
  "Un artefact naît d'un appel d'outil, et le gabarit de":
    "An artifact comes from a tool call, and the template of",
  "Un dossier de travail par conversation": "One working folder per conversation",
  "Un lien de connexion demande à cette application de dialoguer avec le serveur suivant. Vos conversations et vos modèles passeront par lui tant que la connexion sera active.":
    "A connection link asks this app to talk to the following server. Your conversations and models will go through it while the connection is active.",
  "Un modèle dédié — un Qwen 1,5 à 3 milliards, un Gemma compact — répond mieux à ces questions courtes, mais le moteur n'en tient qu'un à la fois : chaque titre sort le modèle de conversation de la mémoire, charge celui-ci, puis recharge l'autre.":
    "A dedicated model — a 1.5 to 3 billion Qwen, a compact Gemma — answers these short questions better, but the engine holds only one at a time: each title unloads the chat model, loads this one, then reloads the other.",
  "Un morph fictif en pre-release, pour montrer le choix de version.":
    "A fictional pre-release morph, to show version choice.",
  "Un morph fictif, pour montrer une fiche du catalogue.":
    "A fictional morph, to show a catalog entry.",
  "Un noyau change le cerveau de l'agent (mémoire, skills, Home Assistant…) pour les conversations qui l'utilisent — le noyau Locaryn n'est jamais remplacé. Les messages d'une session à noyau partent chez":
    "A core changes the agent's brain (memory, skills, Home Assistant…) for the conversations that use it — the Locaryn core is never replaced. Messages of a core session go to",
  "Un ou plusieurs adaptateurs LoRA sont classés NSFW / sans garde-fous. Acceptez la responsabilité avant de sauvegarder. [demo]":
    "One or more LoRA adapters are classified NSFW / unguarded. Accept responsibility before saving. [demo]",
  "Un outil attend votre accord. C'est le seul cas où ne pas être prévenu arrête le travail : l'agent reste en attente tant que personne ne répond.":
    "A tool is waiting for your approval. It's the only case where not being notified stops the work: the agent waits as long as nobody answers.",
  'Un petit modèle "draft" génère des tokens, le grand modèle les valide. ×2 vitesse de génération.':
    'A small "draft" model generates tokens, the big model validates them. ×2 generation speed.',
  "Un rôle, ses consignes, ses conversations": "A role, its instructions, its conversations",
  "Un serveur porte déjà ce nom : retirez-le d'abord, ou renommez-le dans le JSON.":
    "A server already has this name: remove it first, or rename it in the JSON.",
  "Un seul moteur couvre toutes ces fonctions. Chaque capacité active dépend du runtime et des modèles installés.":
    "A single engine covers all these features. Each active capability depends on the runtime and installed models.",
  "Un seul outil qui gère lui-même ses moteurs (llama.cpp + stable-diffusion.cpp), exécute vos modèles GGUF en local et reste privé par défaut. Pas d'API externe, pas de service à assembler.":
    "A single tool that manages its own engines (llama.cpp + stable-diffusion.cpp), runs your GGUF models locally and stays private by default. No external API, no service to assemble.",
  "Un titre que vous écrivez vous-même n'est jamais remplacé.":
    "A title you write yourself is never replaced.",
  "Une bannière du système arrive par-dessus ce que vous faites. Elle est utile pour ce qui se termine hors de l'écran, et pesante pour le reste — d'où ces réglages.":
    "A system banner appears over what you're doing. It's useful for what finishes off screen, and annoying for the rest — hence these settings.",
  "Une carte du catalogue décrit une possibilité ; elle n'est comptée ici qu'après l'enregistrement d'un connecteur MCP.":
    "A catalog card describes a possibility; it only counts here once an MCP connector is saved.",
  "Une erreur technique s'est produite lors de l'exécution : réessaye l'étape.":
    "A technical error happened during execution: retry the step.",
  "Une fiche par chose décidée — un format de rendu, une contrainte de mesure, une exigence de relecture, une commande à lancer. Locaryn les relit avant de répondre, et la portée dit qui les voit.":
    "One note per decision — an output format, a measurement constraint, a review requirement, a command to run. Locaryn rereads them before answering, and the scope says who sees them.",
  "Une fiche par sujet — vous, vos centres d'intérêt, vos projets, les personnes que vous mentionnez. Cliquez une fiche pour voir ce qu'elle contient en entier.":
    "One note per topic — you, your interests, your projects, the people you mention. Click a note to see all it contains.",
  "Une figure a besoin d'un nom et de consignes. Le reste est facultatif.":
    "A figure needs a name and instructions. The rest is optional.",
  "Une ligne de commande dans le dossier de la conversation.":
    "A command line in the conversation's folder.",
  "Une par ligne.": "One per line.",
  "Une partie du paquet ne peut pas fonctionner ici. Le détail est indiqué après l'installation.":
    "Part of the package cannot work here. Details are shown after installation.",
  "Une question pour voir ce qui remonte…": "A question to see what comes up…",
  "Une réponse, un workflow ou une génération s'achève alors que vous êtes ailleurs.":
    "An answer, a workflow or a generation finishes while you're elsewhere.",
  "Une tâche finie en trois secondes ne mérite pas de bannière : seule celle qu'on a eu le temps d'oublier la mérite. Un échec passe outre ce seuil — c'est ce qu'on veut savoir tout de suite.":
    "A task finished in three seconds doesn't deserve a banner: only one you had time to forget does. A failure bypasses this threshold — that's what you want to know right away.",
  "Une vraie page web, que le modèle peut aussi lire et utiliser.":
    "A real web page, which the model can also read and use.",
  "Utiliser ce modèle dans le Chat": "Use this model in the Chat",
  "Utiliser dans le chat": "Use in chat",
  "Utiliser pour la conversation": "Use for the conversation",
  "Utilisez cette machine depuis n'importe où. Le plugin Remote ouvre un tunnel sortant vers un relais : rien n'est ouvert sur votre box.":
    "Use this machine from anywhere. The Remote plugin opens an outgoing tunnel to a relay: nothing is opened on your router.",
  "Variables d'environnement : les valeurs restent masquées, ce sont souvent des clés d'API":
    "Environment variables: values stay hidden, they are often API keys",
  "Variables d'environnement et démarrage automatique de chaque serveur déclaré par l'extension. La commande, l'URL et le transport viennent du fichier du plugin et ne sont pas modifiables ici.":
    "Environment variables and automatic start of each server declared by the extension. The command, URL and transport come from the plugin file and can't be edited here.",
  "Vecteurs pour la recherche augmentée": "Vectors for augmented retrieval",
  "Version, licences, système": "Version, licenses, system",
  "Veuillez renseigner l'adresse du serveur, l'identifiant et le mot de passe.":
    "Please fill in the server address, login and password.",
  "Veuillez saisir un nom d'utilisateur.": "Please enter a username.",
  "Vide la vue (l'historique est conservé)": "Clears the view (history is kept)",
  "Vider le cache local et rafraîchir la liste": "Clear the local cache and refresh the list",
  Vidéo: "Video",
  "Vision (images)": "Vision (images)",
  "Vitesse Modérée": "Moderate speed",
  "Vitesse déduite de la bande passante mémoire, cache à moitié plein.":
    "Speed derived from memory bandwidth, cache half full.",
  "Voir ce qui est envoyé au modèle": "See what is sent to the model",
  "Voir le code source": "View source code",
  "Voir le détail": "See details",
  "Voir le journal complet des modifications": "See the full changelog",
  "Voir les outils que le modèle peut appeler, et en interdire certains":
    "See the tools the model can call, and forbid some",
  "Voir les versions": "See versions",
  "Voir les versions sur GitHub": "See versions on GitHub",
  "Voir toutes les variantes du dépôt avant de choisir":
    "See all the repository's variants before choosing",
  "Voix en cours de synthèse": "Voice being synthesized",
  "Votre IA tourne sur cet ordinateur": "Your AI runs on this computer",
  "Votre PC :": "Your PC:",
  "Votre clé": "Your key",
  "Votre espace de compte et de profil. En mode local, vos conversations, modèles et réglages restent sur cette machine.":
    "Your account and profile space. In local mode, your conversations, models and settings stay on this machine.",
  "Votre téléphone, en un scan": "Your phone, in one scan",
  "Voulez-vous vraiment supprimer cette conversation ?":
    "Do you really want to delete this conversation?",
  Vous: "You",
  "Vous activez": "You are enabling",
  "Vous décidez de ce qu'il peut faire": "You decide what it can do",
  "Vous regardez déjà : la fenêtre montre la même chose. Décochez si vous travaillez sur plusieurs écrans.":
    "You're already watching: the window shows the same thing. Untick if you work on several screens.",
  "Vous utilisez la version": "You are using version",
  "Vous vous engagez à respecter": "You commit to respecting",
  "Vous êtes": "You are",
  "Vérification de la mémoire…": "Checking memory…",
  "Vérification de mise à jour impossible :": "Update check failed:",
  "Vérification du port 7443": "Checking port 7443",
  Vérifier: "Check",
  "Vérifier le CSS associé": "Check the related CSS",
  "Vérifier les mises à jour": "Check for updates",
  "Vérifier que tout fonctionne": "Check that everything works",
  "ZONE COMPTE": "ACCOUNT AREA",
  activés: "enabled",
  ajouté: "added",
  "applique une compression 4-bit uniforme (plus ancienne et un peu moins précise).":
    "applies uniform 4-bit compression (older and a bit less precise).",
  arrêté: "stopped",
  "aucun composant": "no component",
  "aucun — le moteur tourne sur le processeur. Mettez à jour le pilote graphique (Vulkan), puis relancez Locaryn.":
    "none — the engine runs on the CPU. Update the graphics driver (Vulkan), then restart Locaryn.",
  "avec un modèle": "with a model",
  "branche détachée": "detached branch",
  bêta: "beta",
  "ce modèle": "this model",
  "ce projet": "this project",
  "cette conversation": "this conversation",
  "charge et on ajuste leur intensité à chaud": "load and tune their strength live",
  "checkpoint(s) Transformers et ne sera pas installé. Locaryn peut ouvrir automatiquement":
    "Transformers checkpoint(s) and will not be installed. Locaryn can automatically open",
  cœurs: "cores",
  "dans la colonne de gauche. Le code apparaîtra dès que le service sera en écoute.":
    "in the left column. The code will appear as soon as the service is listening.",
  "demande des autorisations": "asks for permissions",
  "depuis l'icône du tray pour arrêter les deux.": "from the tray icon to stop both.",
  "des instructions du serveur. Il est enregistré dans": "server instructions. It is saved in",
  "deux tentatives au plus": "two attempts at most",
  "distillés ou débridés existent publiquement et s'exécutent tels quels dans Locaryn.":
    "distilled or unlocked ones exist publicly and run as is in Locaryn.",
  "doit aussi être installé ici": "must also be installed here",
  "dossier par défaut": "default folder",
  "du contenu généré et de son usage.": "for the generated content and its use.",
  "décline toute responsabilité": "declines all responsibility",
  défaut: "default",
  déjà: "already",
  "emplacement par défaut": "default location",
  "en écoute": "listening",
  enregistré: "saved",
  "est disponible.": "is available.",
  "est prête. Redémarrez pour l'appliquer.": "is ready. Restart to apply it.",
  "est souvent le meilleur choix pragmatique.": "is often the best pragmatic choice.",
  "et les droits des tiers.": "and the rights of third parties.",
  "ex. Ticket #1234, maintenance planifiée…": "e.g. Ticket #1234, scheduled maintenance…",
  "ex: Serveur Prod": "e.g. Prod Server",
  "existe déjà": "already exists",
  "exposent des outils, contextes et accès de données externes (bases de données, fichiers, APIs distantes) directement aux modèles d'IA via le standard Model Context Protocol. Contrairement aux":
    "expose tools, contexts and external data access (databases, files, remote APIs) directly to AI models through the Model Context Protocol standard. Unlike",
  "extension introuvable": "extension not found",
  "fournissent des compétences et consignes compatibles (Claude Code, Gemini CLI, OpenCode). Les":
    "provide compatible skills and instructions (Claude Code, Gemini CLI, OpenCode). The",
  "fournissent un moteur d'agent et une mémoire persistante alternatifs. Les serveurs et outils de données se gèrent dans « Connecteurs & MCP ».":
    "provide an alternative agent engine and persistent memory. Data servers and tools are managed in “Connectors & MCP”.",
  "fournisseur, pas chez le vôtre.": "provider, not yours.",
  "gardez le modèle de ce PC pour la conversation, et demandez-lui de lancer un sous-agent sur l'autre (« lance un sous-agent sur":
    "keep this PC's model for the conversation, and ask it to launch a sub-agent on the other (“launch a sub-agent on",
  génération: "generation",
  "hors dossier": "outside folder",
  "infos, sans que vous ayez à les recoller à chaque fois.":
    "info, without you pasting it each time.",
  "installé avec ce lien": "installed with this link",
  "installé avec ce lien (client + autorité)": "installed with this link (client + authority)",
  installée: "installed",
  "jusqu'au bout": "to the end",
  "l'application ou l'installation en cours": "the current app or installation",
  "le dossier de travail": "the working folder",
  "le défaut": "the default",
  "le moteur n'a pas répondu": "the engine did not answer",
  "le service n'a pas répondu en 20 secondes — un modèle est peut-être en cours de chargement":
    "the service didn't answer within 20 seconds — a model may be loading",
  "les créateurs du modèle": "the model's creators",
  "les lois applicables": "applicable laws",
  "llama.cpp exécute des modèles, il ne les entraîne pas. Ces opérations demandent PyTorch et un GPU adapté ; le résultat (GGUF) se charge ensuite ici.":
    "llama.cpp runs models, it doesn't train them. These operations need PyTorch and a suitable GPU; the result (GGUF) is then loaded here.",
  "longue réflexion": "long reasoning",
  "message de ce projet, Locaryn retrouve automatiquement les passages utiles et les glisse dans la question — le modèle répond avec":
    "message in this project, Locaryn automatically finds the useful passages and slips them into the question — the model answers with",
  modifié: "modified",
  modèle: "model",
  "modèle de chat actif": "active chat model",
  "modèle inconnu": "unknown model",
  modèles: "models",
  "modèles affichés": "models shown",
  "modèles trop lourds": "models too heavy",
  "modèles via la RAM": "models via RAM",
  "modèles à l'aise": "models that fit comfortably",
  "n'en prévoit aucun : il répondra en prose sans rien produire ici. Ce panneau reste utilisable pour exécuter un bloc de code à la main.":
    "provides none: it will answer in prose without producing anything here. This panel stays usable to run a code block by hand.",
  "n'est pas ouvert : ses outils ne répondront pas. Ouvrez":
    "is not open: its tools won't answer. Open",
  "ne déclare pas de gabarit de conversation dans son fichier : impossible de dire d'ici s'il gère les outils. Chargez-le et la réponse deviendra certaine — le moteur dira alors quel gabarit il a retenu.":
    "declares no chat template in its file: impossible to tell from here whether it handles tools. Load it and the answer will become certain — the engine will then say which template it used.",
  "non installé": "not installed",
  "non revendiqué": "unclaimed",
  "non vérifié": "not verified",
  "offre 99.5% de la qualité de": "offers 99.5% of the quality of",
  "ordinateur et pas à un autre.": "computer and not another one.",
  paramètres: "parameters",
  "pas de progrès": "no progress",
  personnalisé: "custom",
  "pièces jointes": "attachments",
  "pour ce qui résiste": "for what resists",
  "pour confirmer :": "to confirm:",
  "pour les usages qui en sont faits.": "for the uses made of it.",
  "pour… »). Le sous-agent calcule là-bas, mais ses outils, et donc les MCP, travaillent ici.":
    "for…”). The sub-agent computes there, but its tools, and so the MCPs, work here.",
  "presse-papier image indisponible": "image clipboard unavailable",
  "qui produit les embeddings (zéro configuration). Pour une récupération nettement plus fine, installez un vrai modèle d'":
    "which produces the embeddings (zero setup). For much finer retrieval, install a real model of",
  renommé: "renamed",
  "réflexion courte": "short reasoning",
  réfléchir: "think",
  répète: "repeats",
  résultat: "result",
  résumé: "summary",
  "s ; chaque code ne sert qu'une fois.": "s; each code is single-use.",
  "s'appliquera au prochain chargement.": "will apply at the next load.",
  "s'ouvre directement. Nouveau code dans": "opens directly. New code in",
  "sans garde-fous": "unguarded",
  "sans pénalité de répétition": "no repetition penalty",
  "son dossier": "its folder",
  supprimé: "deleted",
  "sur disque": "on disk",
  "tente de se connecter": "is trying to connect",
  terminé: "finished",
  "tokens, la fenêtre actuelle n'en offre que": "tokens, the current window only offers",
  "toujours prévenir": "always notify",
  "tout en économisant ~20% d'espace mémoire. Si vous voulez la qualité maximale,":
    "while saving ~20% of memory. If you want maximum quality,",
  "transforment la structure de l'application (injection d'espaces Studio, barres d'outils, slots et moteurs IA dédiés). Les":
    "transform the app's structure (Studio spaces, toolbars, slots and dedicated AI engines). The",
  "une adresse inconnue": "an unknown address",
  utilisé: "used",
  '{ "API_KEY": "votre-clé" }': '{ "API_KEY": "your-key" }',
  "· compétences d'agent & règles": "· agent skills & rules",
  "· début seulement": "· beginning only",
  "· formats supportés :": "· supported formats:",
  "· moteur à mettre à niveau (au prochain démarrage)": "· engine to upgrade (at next start)",
  "· mémoire & agent alternatif": "· memory & alternative agent",
  "· métamorphose UI & moteur IA": "· UI metamorphosis & AI engine",
  "· outils exposés à l'agent": "· tools exposed to the agent",
  "· pont de données ou service": "· data bridge or service",
  "» · envoyé automatiquement au modèle": "” · sent automatically to the model",
  "À choisir uniquement si votre machine dispose d'une grande marge de mémoire.":
    "Choose only if your machine has a large memory margin.",
  "À installer sur cet appareil": "To install on this device",
  "À jour :": "Up to date:",
  "À propos": "About",
  "À quoi ça sert ?": "What is it for?",
  "À éviter sauf besoin spécifique d'archivage ou conversion.":
    "Avoid unless you specifically need archiving or conversion.",
  "Écarter la question": "Dismiss the question",
  "Écarter sans répondre — le modèle continuera avec l'option la plus prudente":
    "Dismiss without answering — the model will continue with the most cautious option",
  "Échantillon de voix à cloner": "Voice sample to clone",
  "Échantillonnage, fenêtre de contexte, permissions de la conversation.":
    "Sampling, context window, conversation permissions.",
  Échap: "Esc",
  Échec: "Failed",
  Économe: "Frugal",
  Écosystème: "Ecosystem",
  "Écouter la note vocale": "Listen to the voice note",
  "Écris des tests unitaires pour la logique principale": "Write unit tests for the main logic",
  "Édition & optimisation de modèle": "Model editing & optimization",
  "Édition modèle": "Model editing",
  Éjecter: "Eject",
  Élevé: "High",
  Éphémère: "Ephemeral",
  "Éphémère actif (cliquer pour quitter)": "Ephemeral on (click to leave)",
  "Époques (Epochs)": "Epochs",
  "Équilibre parfait vitesse / intelligence.": "Perfect balance of speed / intelligence.",
  "Équilibre vitesse / qualité": "Speed / quality balance",
  Éteinte: "Off",
  "à l'instant": "just now",
  "à transférer. Sur le même disque c'est instantané ; vers un autre disque, comptez plusieurs minutes.":
    "to transfer. On the same disk it's instant; to another disk, allow several minutes.",
  échec: "failure",
  "échec d'une étape": "a step failed",
  "éjection dans": "ejecting in",
  étapes: "steps",
  "état inconnu": "unknown state",
  "÷4 VRAM (max réel)": "÷4 VRAM (real max)",
  "— AirLLM actif : chaque modèle est exécutable en local": "— AirLLM on: every model runs locally",
  "— choisissez la qualité": "— choose the quality",
  "— compression maximale réelle du cache sous llama.cpp (":
    "— real maximum cache compression under llama.cpp (",
  "— compte requis": "— account required",
  "— conversion requise pour llama.cpp": "— conversion required for llama.cpp",
  "— le certificat peut aussi s'installer à la main dans Réglages → Connexion.":
    "— the certificate can also be installed by hand in Settings → Connection.",
  "— noms de produits et termes techniques :": "— product names and technical terms:",
  "— non installé": "— not installed",
  "— réglages fournis par l'extension elle-même.": "— settings provided by the extension itself.",
  "— triés du plus adapté au plus lourd": "— sorted from best fit to heaviest",
  "— une seule variante sera téléchargée, pas tout le dépôt.":
    "— a single variant will be downloaded, not the whole repository.",
  "ⓘ Par défaut, c'est votre": "ⓘ By default, it's your",
  "▲ Masquer les variantes": "▲ Hide variants",
  "(facultative)": "(optional)",
  ") comme": ") as",
  "+ Nouvel identifiant": "+ New login",
  "0 Go (CPU Seul)": "0 GB (CPU only)",
  Absente: "Missing",
  Accepte: "Accepts",
  Activez: "Enable",
  "Actualisation…": "Refreshing…",
  "Adaptateurs LoRA": "LoRA adapters",
  Administrateur: "Administrator",
  "Adresse HTTP": "HTTP address",
  "Adresse de l'autre PC": "Other PC's address",
  "Adresse, par ex. 192.168.0.57:7474": "Address, e.g. 192.168.0.57:7474",
  "Affichage en Grille / Cartes": "Grid / card view",
  "Ajout…": "Adding…",
  "Analyse Perf PC": "PC performance scan",
  "Analyser mon PC": "Scan my PC",
  "Apache-2.0 · module entreprise BSL-1.1": "Apache-2.0 · enterprise module BSL-1.1",
  Appairage: "Pairing",
  "Appairage par QR": "QR pairing",
  "Appairage…": "Pairing…",
  Appliquer: "Apply",
  "Apporte par": "Provided by",
  Archives: "Archives",
  Arguments: "Arguments",
  Arrete: "Stopped",
  Arreter: "Stop",
  "Arret…": "Stopping…",
  "Assistant IA Locaryn": "Locaryn AI Assistant",
  Autoriser: "Allow",
  Autre: "Other",
  "Base SQLite": "SQLite database",
  "Base de connaissances (RAG)": "Knowledge base (RAG)",
  "Calcul…": "Computing…",
  Catalogue: "Catalog",
  "Ce qu'il faut savoir": "What to know",
  "Ce que Locaryn retient": "What Locaryn remembers",
  "Ce que fait ce moteur": "What this engine does",
  "Centre de notifications": "Notification center",
  Certificat: "Certificate",
  "Ces versions (ex:": "These versions (e.g.",
  "Changer l'avatar": "Change avatar",
  "Chargement par mmap": "mmap loading",
  "Chargement…": "Loading…",
  Chercher: "Search",
  "Code QR d'appairage": "Pairing QR code",
  "Code d'appairage": "Pairing code",
  Commande: "Command",
  "Commande locale (npx, uvx, python…)": "Local command (npx, uvx, python…)",
  Commandes: "Commands",
  Commencer: "Start",
  "Compatibles d'abord": "Compatible first",
  "Compression…": "Compressing…",
  Compris: "Got it",
  "Comptes actifs": "Active accounts",
  Configurer: "Configure",
  Connecteur: "Connector",
  "Connecteur MCP": "MCP connector",
  "Connecteurs MCP": "MCP connectors",
  "Connecteurs MCP (": "MCP connectors (",
  Connexions: "Connections",
  Consignes: "Instructions",
  Contexte: "Context",
  "Couches Cibles (Layer Range)": "Target layers (Layer Range)",
  "Couches GPU": "GPU layers",
  "Couches cibles": "Target layers",
  "Couleur d'accentuation": "Accent color",
  "Date de sortie officielle": "Official release date",
  "Dernier message": "Last message",
  Disques: "Disks",
  Distant: "Remote",
  "Documents (RAG)": "Documents (RAG)",
  "Dossiers locaux": "Local folders",
  "Droits Administrateur": "Administrator rights",
  Effacer: "Clear",
  "Effacer l'index": "Erase index",
  "Emplacements SSH (": "SSH locations (",
  "En cours": "In progress",
  "En cours d'utilisation": "In use",
  "En cours ·": "In progress ·",
  "En marche": "Running",
  "En pause": "Paused",
  "Enregistrement…": "Saving…",
  "Envoi…": "Sending…",
  "Espace de travail": "Workspace",
  "Espaces de travail": "Workspaces",
  "Familles de connecteurs": "Connector families",
  "File d'attente locale (llama-server)": "Local queue (llama-server)",
  "Filtrer par marque": "Filter by brand",
  "Filtrer…": "Filter…",
  "Format de rendu, Contraintes de mesure, Relecture…":
    "Output format, Measurement constraints, Review…",
  Formulaire: "Form",
  "Fournisseur distant ·": "Remote provider ·",
  Go: "GB",
  "Go RAM": "GB RAM",
  "Go VRAM": "GB VRAM",
  "Go.": "GB.",
  Grille: "Grid",
  Identifiant: "Login",
  Ignorer: "Ignore",
  "Indexation de": "Indexing",
  "Indexation…": "Indexing…",
  Indexer: "Index",
  "Installation :": "Installation:",
  "Installation…": "Installing…",
  "Installation… (quelques minutes)": "Installing… (a few minutes)",
  "Jamais traduits": "Never translated",
  "Largeur de l'espace de travail": "Workspace width",
  "Lecture indisponible": "Playback unavailable",
  "Lecture…": "Reading…",
  Libre: "Free",
  Liste: "List",
  "Manifeste :": "Manifest:",
  Membre: "Member",
  "Messages en attente": "Queued messages",
  "Mode d'ajout": "Add mode",
  "Modele :": "Model:",
  Modifications: "Changes",
  "Moteurs IA & Runtimes actifs (": "Active AI engines & runtimes (",
  "N couches": "N layers",
  Navigation: "Navigation",
  Nom: "Name",
  "Nom (A → Z)": "Name (A → Z)",
  "Nom / Alias": "Name / Alias",
  "Nom d'utilisateur (Identifiant)": "Username (Login)",
  "Note d'audit (optionnel)": "Audit note (optional)",
  "Note vocale": "Voice note",
  "Nouvel onglet": "New tab",
  Noyau: "Core",
  "Noyau :": "Core:",
  "Noyaux (Cores)": "Cores",
  "Noyaux alternatifs": "Alternative cores",
  Objectif: "Goal",
  Occupation: "Usage",
  "Offload experts MoE → CPU": "Offload MoE experts → CPU",
  "Onglets de l'espace de travail": "Workspace tabs",
  Oublier: "Forget",
  "Ouverture…": "Opening…",
  "PC Portable (8 Go RAM, CPU 4c)": "Laptop (8 GB RAM, 4-core CPU)",
  "PC de jeu (16 Go RAM, 8 Go VRAM)": "Gaming PC (16 GB RAM, 8 GB VRAM)",
  "Page suivante": "Next page",
  Passer: "Skip",
  "Permissions :": "Permissions:",
  "Permissions par defaut": "Default permissions",
  "Plus populaires": "Most popular",
  "Plus tard": "Later",
  Poids: "Weights",
  Processeur: "CPU",
  "Protocole Transport": "Transport protocol",
  Précédent: "Previous",
  "Présentation de Locaryn": "Locaryn introduction",
  Raisonnement: "Reasoning",
  "Raisonnement (": "Reasoning (",
  Ranger: "File",
  "Rechargement…": "Reloading…",
  Recharger: "Reload",
  Redemarrer: "Restart",
  Refuser: "Refuse",
  "Relais Remote": "Remote relay",
  Relecteur: "Reviewer",
  Relire: "Review",
  "Replier ce message": "Collapse this message",
  Ressortir: "Restore",
  Retoucher: "Edit",
  Revenir: "Back",
  "Runtime IA": "AI runtime",
  "S'appairer": "Pair",
  "Sauvegarde…": "Saving…",
  Sortie: "Output",
  "Sources de catalogue": "Catalog sources",
  Sujet: "Topic",
  "Taille de Batch": "Batch size",
  "Tailles disponibles": "Available sizes",
  Tapez: "Type",
  "Taux d'apprentissage (LR)": "Learning rate (LR)",
  "Threads CPU": "CPU threads",
  "Types de paquets": "Package types",
  Utilisateur: "User",
  Utiliser: "Use",
  "VRAM Carte Graphique": "Graphics card VRAM",
  "Variables d'environnement (JSON, optionnel)": "Environment variables (JSON, optional)",
  "Versions disponibles": "Available versions",
  Vider: "Empty",
  "Vue de l'historique": "History view",
  "Zone dangereuse": "Danger zone",
  "adresse HTTP": "HTTP address",
  "commande locale": "local command",
  "commande…": "command…",
  "composant(s) illisible(s) :": "unreadable component(s):",
  "disponibles)": "available)",
  "en attente…": "waiting…",
  "en cours": "in progress",
  "en cours…": "in progress…",
  "en ligne": "online",
  "ex: gemma4:e2b, qwen3:8b, hf.co/user/repo, https://huggingface.co/...":
    "e.g. gemma4:e2b, qwen3:8b, hf.co/user/repo, https://huggingface.co/...",
  "ex: marie, admin...": "e.g. marie, admin...",
  "fluides GPU": "smooth on GPU",
  "lecture seule": "read-only",
  "owner/repo · https://github.com/… · ./mon-plugin":
    "owner/repo · https://github.com/… · ./my-plugin",
  "par M": "by M",
  "reste en place": "stays in place",
  "seul responsable": "solely responsible",
  "trop lourds": "too heavy",
  "version inconnue": "unknown version",
  Étapes: "Steps",
  "2-bit Extrême": "2-bit Extreme",
  "4-bit ARM (format retiré)": "4-bit ARM (retired format)",
  "Ancien réarrangement de Q4_0 pour les processeurs ARM. Les versions récentes de llama.cpp ne le chargent plus : prenez Q4_0, réarrangé automatiquement au chargement.":
    "Old Q4_0 repacking for ARM processors. Recent llama.cpp versions no longer load it: pick Q4_0, repacked automatically at load time.",
  "Format obsolète.": "Obsolete format.",
  "4-bit uniforme avec offset pour une fidélité légèrement supérieure à Q4_0.":
    "Uniform 4-bit with offset for slightly higher fidelity than Q4_0.",
  "6-bit K-Quant (Qualité / RAM)": "6-bit K-Quant (Quality / RAM)",
  "8-bit Pleine Fidélité (Sans perte)": "8-bit Full Fidelity (Lossless)",
  "99.5% de la qualité de Q8_0 tout en économisant ~20% de RAM. Souvent le meilleur choix haut de gamme.":
    "99.5% of Q8_0's quality while saving ~20% RAM. Often the best high-end choice.",
  "Accéder au réseau": "Access the network",
  "Activez le mode serveur dans les réglages et scannez le code avec l'application mobile : vous retrouvez vos modèles et vos conversations sur le même Wi-Fi.":
    "Enable server mode in settings and scan the code with the mobile app: you get your models and conversations on the same Wi-Fi.",
  "Ajoutez des capacités : image, voix, contrôle de l'ordinateur, ou le serveur MCP d'un logiciel. Le modèle peut les installer lui-même — avec votre accord, sauf au niveau « Tout autoriser ».":
    "Add capabilities: image, voice, computer control, or a software's MCP server. The model can install them itself — with your consent, except at the “Allow everything” level.",
  "Aucun compte n'existe.": "No account exists.",
  "Autre réponse": "Other answer",
  "Cet outil écrit dans un fichier du projet": "This tool writes to a project file",
  "Chemin du dossier projet:": "Project folder path:",
  "Chirurgie des poids (RepE / transformer-lens), en Python.":
    "Weight surgery (RepE / transformer-lens), in Python.",
  "Choisissez le dossier avant d'envoyer le premier message : le modèle y lit et y écrit. Sans dossier, la conversation est libre et range ses fichiers à part.":
    "Choose the folder before sending the first message: the model reads and writes there. Without a folder, the conversation is free and keeps its files apart.",
  "Compression 4-bit guidée par matrice d'importance (imatrix) pour une précision supérieure à Q4_0.":
    "4-bit compression guided by an importance matrix (imatrix) for better precision than Q4_0.",
  "Compression extrême (1 à 2 bits) calibrée par imatrix pour faire tourner de gros modèles sur 4-6 Go de RAM.":
    "Extreme compression (1 to 2 bits) calibrated by imatrix to run big models on 4-6 GB of RAM.",
  "Compression maximale en 2-bit (dégradation possible, réservé aux tests sur matériel très limité).":
    "Maximum 2-bit compression (possible degradation, for tests on very limited hardware).",
  "Compromis pour réduire l'empreinte mémoire sous les 8 Go tout en gardant des phrases bien structurées.":
    "Trade-off to keep the memory footprint under 8 GB while keeping well-structured sentences.",
  "Compte et profil": "Account and profile",
  "Connecteurs et MCP": "Connectors and MCP",
  "De « Aperçu seul » à « Tout autoriser », cinq niveaux. Quand il doit demander, un bandeau apparaît au-dessus du champ : autorisez une fois, pour la session ou pour toujours.":
    "From “Preview only” to “Allow everything”, five levels. When it must ask, a banner shows above the field: allow once, for the session or forever.",
  "Démarrage du téléchargement...": "Starting download...",
  "Déploiement en un clic de vos applications et suivi des logs de production.":
    "One-click deployment of your apps and production log tracking.",
  "Enregistrer des serveurs LSP": "Register LSP servers",
  "Entraîne un modèle élève depuis un professeur : pile d'entraînement Python.":
    "Trains a student model from a teacher: Python training stack.",
  "Entraînement & Oblitération": "Training & Obliteration",
  "Excellente fidélité avec ~15% de mémoire en moins que Q6_K.":
    "Excellent fidelity with ~15% less memory than Q6_K.",
  "Exécuter des commandes": "Run commands",
  "Exécution sécurisée de scripts Python, calculs scientifiques et génération de graphiques en bac à sable.":
    "Safe execution of Python scripts, scientific computing and chart generation in a sandbox.",
  "Gestion des conteneurs locaux/distants, inspection des logs et commandes kubectl assistées.":
    "Local/remote container management, log inspection and assisted kubectl commands.",
  "Graphe de Mémoire & RAG": "Memory Graph & RAG",
  Général: "General",
  "Génération & Animation vidéo": "Video Generation & Animation",
  "Génération & Édition d'images": "Image Generation & Editing",
  "Génération musicale": "Music generation",
  "Gérer d'autres extensions": "Manage other extensions",
  "I-Matrix 3-bit Calibré": "Calibrated 3-bit I-Matrix",
  "I-Matrix 4-bit Calibré": "Calibrated 4-bit I-Matrix",
  "Image copiée": "Image copied",
  "Image enregistrée": "Image saved",
  "Inférence Très Grands Modèles (70B+)": "Very Large Model Inference (70B+)",
  "Installation du moteur AirLLM (pip install airllm)…":
    "Installing the AirLLM engine (pip install airllm)…",
  "Installez cloudflared (winget install Cloudflare.cloudflared). Aucun compte requis.":
    "Install cloudflared (winget install Cloudflare.cloudflared). No account required.",
  "Interrompu avant la fin.": "Interrupted before the end.",
  "Intégration Merge Requests, pipelines CI/CD et parcours de projets sur votre instance GitLab.":
    "Merge Requests, CI/CD pipelines and project browsing on your GitLab instance.",
  "Intégration Pyright, rust-analyzer, tsserver et gopls pour l'analyse de type et autocomplétion exacte.":
    "Pyright, rust-analyzer, tsserver and gopls integration for type analysis and exact autocompletion.",
  "K-Quant renforcé en précision sur les couches d'attention critiques.":
    "K-Quant with higher precision on critical attention layers.",
  "Le contexte du compte et le contexte partagé passent par un serveur : activez le mode serveur, ou connectez-vous à un serveur existant.":
    "Account context and shared context go through a server: enable server mode, or connect to an existing server.",
  "Le modèle entier sur la carte, le reste en contexte":
    "The whole model on the card, the rest as context",
  "Lecture des spécifications de projet, tickets Jira, tâches et documentation d'équipe.":
    "Reading project specs, Jira tickets, tasks and team documentation.",
  "Les modèles s'exécutent ici, vos conversations restent ici. Le premier message charge le modèle en mémoire : c'est plus long la première fois, puis instantané.":
    "Models run here, your conversations stay here. The first message loads the model into memory: it takes longer the first time, then it's instant.",
  "Lire des variables d'environnement": "Read environment variables",
  "Lire le projet": "Read the project",
  "Lire les fichiers du projet": "Read the project's files",
  "Locaryn attend votre accord": "Locaryn is waiting for your approval",
  "Locaryn installe la passerelle lui-même. Il faut Node.js 22 ou 24.":
    "Locaryn installs the gateway itself. Node.js 22 or 24 is required.",
  "MCP communauté": "Community MCP",
  "MODÈLES & INTELLIGENCE": "MODELS & INTELLIGENCE",
  "Me suit sur mes autres appareils, et reste invisible aux autres personnes du projet. Pour mes exigences à moi.":
    "Follows me on my other devices, and stays invisible to other people on the project. For my own requirements.",
  "Mes Modèles Installés": "My Installed Models",
  "Modifier les fichiers du projet": "Modify the project's files",
  "Modèle 16/32-bit d'origine. Très lourd et inutilement gourmand pour une exécution locale standard.":
    "Original 16/32-bit model. Very heavy and needlessly hungry for standard local execution.",
  Modèles: "Models",
  "Modèles installés": "Installed models",
  "Modélisation 3D": "3D modeling",
  "Morphs et Skills": "Morphs and Skills",
  "Mémoire vectorielle persistante entre les sessions de chat pour conserver le contexte du projet.":
    "Persistent vector memory across chat sessions to keep the project's context.",
  "Ne part jamais d'ici. Pour ce qui n'a de sens que sur ce poste : outils installés, chemins, versions.":
    "Never leaves here. For what only makes sense on this machine: installed tools, paths, versions.",
  "Nom du projet:": "Project name:",
  "Notifications en direct et import d'extraits de discussions d'équipe dans l'agent.":
    "Live notifications and import of team discussion excerpts into the agent.",
  "Oblitération terminée": "Obliteration finished",
  "Obtenue automatiquement au premier démarrage": "Obtained automatically at first start",
  "Outils des connecteurs": "Connector tools",
  "Outils intégrés": "Built-in tools",
  "Ouvrir des aperçus": "Open previews",
  "Paramètres Système": "System Settings",
  "Partout, sans limite de durée. Engagement fort — à réserver aux outils sûrs.":
    "Everywhere, with no time limit. Strong commitment — keep it for safe tools.",
  "Passer en 100 % local": "Go 100 % local",
  Personnalité: "Personality",
  "Poids bruts (Non compressé)": "Raw weights (Uncompressed)",
  "Pour cet appel seulement — la question reviendra la prochaine fois.":
    "For this call only — the question will come back next time.",
  "Pour tous les appels de cet outil, jusqu'à la fermeture de Locaryn.":
    "For every call of this tool, until Locaryn is closed.",
  "Prenez une conversation dans la liste et déposez-la sur la corbeille pour l'archiver, sur un projet pour l'y ranger, ou sur une autre pour les réunir. Lâchée ailleurs, elle reprend sa place.":
    "Take a conversation from the list and drop it on the trash to archive it, on a project to file it there, or on another to merge them. Dropped elsewhere, it goes back to its place.",
  "Processeur seul, la carte reste libre": "CPU only, the card stays free",
  "Prompt système": "System prompt",
  Préférences: "Preferences",
  "Quantification 3-bit calibrée par imatrix : préserve la cohérence textuelle bien mieux que Q3_K classique.":
    "3-bit quantization calibrated by imatrix: keeps text coherence much better than classic Q3_K.",
  "Quantification 3-bit ultra-légère pour machines très contraintes en mémoire.":
    "Ultra-light 3-bit quantization for very memory-constrained machines.",
  "Quantification 4-bit uniforme simple. Très rapide et universelle, mais un peu moins fine que Q4_K_M.":
    "Simple uniform 4-bit quantization. Very fast and universal, but a bit less precise than Q4_K_M.",
  "Quelque chose est cassé dans une conversation": "Something is broken in a conversation",
  "Qui doit voir que le rendu final est en A2 ?": "Who should see that the final output is A2?",
  "Raisonnement accru (logique, code, maths). Qualité quasi indiscernable du modèle complet.":
    "Stronger reasoning (logic, code, maths). Quality nearly indistinguishable from the full model.",
  "Recherche de dépôts, gestion des issues, Pull Requests et suivi des workflows GitHub Actions.":
    "Repository search, issue management, Pull Requests and GitHub Actions workflow tracking.",
  "Recherche web en direct et extraction automatique de documentation technique à jour.":
    "Live web search and automatic extraction of up-to-date technical documentation.",
  "Recherche web et locale via l'API Brave (remplace le serveur de référence archivé).":
    "Web and local search through the Brave API (replaces the archived reference server).",
  "Recommandé (K-Quant Medium)": "Recommended (K-Quant Medium)",
  "Reproduction parfaite et sans aucune altération du modèle original. Exige beaucoup de RAM/VRAM.":
    "Perfect reproduction of the original model without any alteration. Needs a lot of RAM/VRAM.",
  "Retenu : tout appel de cet outil dans ce projet passera sans question.":
    "Remembered: every call of this tool in this project will go through without asking.",
  "Risque élevé": "High risk",
  "Rétropropagation — nécessite une pile Python (Unsloth, PEFT).":
    "Backpropagation — needs a Python stack (Unsloth, PEFT).",
  "SYSTÈME & PROFIL": "SYSTEM & PROFILE",
  "Sa réflexion s'affiche en direct puis se replie au-dessus de la réponse. Ouvrez-la pour voir comment il est arrivé à sa conclusion.":
    "Its reasoning shows live then folds above the answer. Open it to see how it reached its conclusion.",
  "Serveur MCP Personnalisé": "Custom MCP Server",
  "Standard d'or recommandé : compresse intelligemment chaque couche selon son importance. Meilleur équilibre vitesse / mémoire / qualité.":
    "Recommended gold standard: smartly compresses each layer according to its importance. Best balance of speed / memory / quality.",
  "Studio d'entraînement": "Training studio",
  "Synthèse vocale & Voix": "Speech synthesis & Voice",
  "Tests E2E, captures d'écran et automation de navigateurs web directement depuis l'agent.":
    "E2E tests, screenshots and web browser automation directly from the agent.",
  "Tâche :": "Task:",
  "Téléchargement terminé": "Download finished",
  "Une conversation attend votre réponse": "A conversation is waiting for your answer",
  "Variante 4-bit plus légère que Q4_K_M : gagne ~10% de RAM avec une qualité quasi identique.":
    "4-bit variant lighter than Q4_K_M: saves ~10% RAM with nearly identical quality.",
  "Variante standard disponible dans le dépôt.": "Standard variant available in the repository.",
  "Veuillez sélectionner un module dans la liste ci-dessus.":
    "Please select a module in the list above.",
  "Visible de tous ceux qui travaillent sur ce projet. Pour ce que le projet a décidé, pas pour une préférence personnelle.":
    "Visible to everyone working on this project. For what the project decided, not for a personal preference.",
  "l'objectif ne progresse plus : le modèle redit la même réponse sans agir. Arrêt — reformulez la demande, ou vérifiez que l'outil dont il a besoin est disponible.":
    "the goal is no longer progressing: the model repeats the same answer without acting. Stopped — rephrase the request, or check that the tool it needs is available.",
  "le modèle chargé ne sait pas analyser d'images": "the loaded model cannot analyze images",
  "sans limite": "no limit",
  "vérifier le résultat": "check the result",
  Équilibré: "Balanced",
  "2 fois": "2 times",
  "3 fois": "3 times",
  "5 fois": "5 times",
  "Aperçu seul": "Preview only",
  Argile: "Clay",
  "Aucune question, jamais : installations et machines distantes comprises. Dangereux — le modèle peut tout faire, y compris installer un logiciel inconnu ou effacer des fichiers. À réserver à une machine ou une tâche où une erreur ne coûte rien.":
    "No questions, ever: installations and remote machines included. Dangerous — the model can do anything, including installing unknown software or deleting files. Keep it for a machine or a task where a mistake costs nothing.",
  Automatique: "Automatic",
  Autonome: "Autonomous",
  "Autorisations, base de connaissances, archivage": "Permissions, knowledge base, archiving",
  Bas: "Low",
  "Base de connaissances": "Knowledge base",
  Calendrier: "Calendar",
  "Ce que fait chaque commande": "What each command does",
  "Cet ordinateur": "This computer",
  "Chargement en cours": "Loading",
  "Compression KV": "KV compression",
  Confiance: "Trusted",
  "Connecteurs & MCP": "Connectors & MCP",
  Converti: "Converted",
  "Diffusion locale (stable-diffusion.cpp)": "Local diffusion (stable-diffusion.cpp)",
  "Effacer l'affichage": "Clear display",
  Entretien: "Maintenance",
  "Fluide GPU": "Smooth GPU",
  "Format classique universel.": "Classic universal format.",
  "Identifiant API (my.telegram.org)": "API ID (my.telegram.org)",
  "Lecture seule : toute écriture et toute commande sont refusées, sans exception.":
    "Read-only: every write and every command is refused, without exception.",
  "Les lectures et les outils annoncés en lecture seule passent sans question. Écrire un fichier, lancer une commande ou appeler un autre outil demande toujours.":
    "Reads and tools declared read-only go through without asking. Writing a file, running a command or calling another tool always asks.",
  "LoRA (adaptateurs)": "LoRA (adapters)",
  "Moi seulement": "Only me",
  "Morphs, Skills & Noyaux": "Morphs, Skills & Cores",
  "Moteur IA": "AI Engine",
  "Moteurs (llama.cpp, sd.cpp)": "Engines (llama.cpp, sd.cpp)",
  Mousse: "Moss",
  Moyen: "Medium",
  Navigateur: "Browser",
  "OK via RAM": "OK via RAM",
  Océan: "Ocean",
  "Offload RAM / experts MoE": "RAM offload / MoE experts",
  Partiel: "Partial",
  Personnes: "People",
  Pierre: "Stone",
  Prudent: "Cautious",
  "Revue de code": "Code review",
  Sauge: "Sage",
  "Skills & Packs d'agent": "Skills & Agent packs",
  "Sous-agent": "Sub-agent",
  Sujets: "Topics",
  "Sécurité maximale : chaque accès aux fichiers, chaque commande et chaque outil demande votre accord. C'est le réglage par défaut.":
    "Maximum safety: every file access, every command and every tool asks for your approval. This is the default setting.",
  "Tout autoriser": "Allow everything",
  "Tout ce qui se passe sur cet ordinateur passe sans question : lectures, écritures, commandes, outils. Deux choses demandent encore : installer quelque chose (un morph, un skill, un connecteur, un logiciel ou un script venu du web, dont rien ne garantit la provenance) et agir sur une autre machine.":
    "Everything happening on this computer goes through without asking: reads, writes, commands, tools. Two things still ask: installing something (a morph, a skill, a connector, a software or a script from the web, whose origin nothing guarantees) and acting on another machine.",
  "Trop lourd": "Too heavy",
  "Tunnels Microsoft": "Microsoft Tunnels",
  "Variante de Q4_0.": "Q4_0 variant.",
  Zones: "Areas",
  "GPU, cache KV, contexte, offload": "GPU, KV cache, context, offload",
  "libéré dans": "freed in",
  Prudence: "Caution",
  "Lecture de la conversation…": "Reading the conversation…",
  "Grande réserve : idéal si la carte affiche aussi le bureau, un navigateur ou un logiciel 3D.":
    "Large reserve: best if the card also drives the desktop, a browser or 3D software.",
  "Réserve moyenne : le bon compromis pour un PC qui fait surtout tourner Locaryn.":
    "Medium reserve: the right trade-off for a PC that mostly runs Locaryn.",
  "Réserve minimale : presque toute la VRAM libre va au modèle. À éviter si d'autres applications utilisent la carte.":
    "Minimal reserve: almost all free VRAM goes to the model. Avoid if other apps use the card.",
  "En mode risqué, si une autre application réclame de la VRAM pendant que le modèle tourne, Windows déplace une partie du modèle en mémoire système : les réponses ralentissent fortement.":
    "In risky mode, if another app claims VRAM while the model runs, Windows moves part of the model to system memory: answers slow down a lot.",
  "Au moment du chargement, Locaryn regarde la VRAM libre à cet instant — pas la VRAM totale de la carte — et la réserve se calcule sur elle : elle grandit avec la mémoire libre, mais de moins en moins vite. Le modèle doit y tenir en laissant libre cette réserve ; sinon il n'est pas chargé, à tous les niveaux, et une fenêtre explique pourquoi (un jeu ouvert, par exemple). Le contexte s'ajuste ensuite à la VRAM qui reste.":
    "When loading, Locaryn looks at the VRAM free at that moment — not the card's total VRAM — and the reserve is computed from it: it grows with free memory, but more and more slowly. The model must fit in it while leaving that reserve free; otherwise it is not loaded, at every level, and a window explains why (a game running, for example). The context then adjusts to the VRAM left.",
  "Au moment du chargement, Locaryn regarde la VRAM libre à cet instant — pas la VRAM totale de la carte. Le modèle doit y tenir en laissant libre la réserve du niveau choisi ; sinon il n'est pas chargé, à tous les niveaux, et une fenêtre explique pourquoi (un jeu ouvert, par exemple). Le contexte s'ajuste ensuite à la VRAM qui reste.":
    "When loading, Locaryn looks at the VRAM free at that moment — not the card's total VRAM. The model must fit in it while leaving the chosen level's reserve free; otherwise it is not loaded, at every level, and a window explains why (a game running, for example). The context then adjusts to the VRAM left.",
  Texte: "Text",
  taille: "size",
  tailles: "sizes",
  Licence: "License",
  famille: "family",
  familles: "families",
  "Multimodal encoder-free (texte, image, audio) avec function calling natif.":
    "Encoder-free multimodal (text, image, audio) with native function calling.",
  "Modèles légers ultra-performants (2B, 9B, 27B).":
    "Ultra-efficient lightweight models (2B, 9B, 27B).",
  "Génération d'août 2026 : agentique, appels d'outils, vision (images), contexte de 262 144 jetons. GGUF llama.cpp (unsloth) ; le projecteur d'images est proposé à part.":
    "August 2026 generation: agentic, tool calls, vision (images), 262,144-token context. llama.cpp GGUF (unsloth); the image projector is offered separately.",
  "Génération d'avril 2026 : agentique, appels d'outils fiables, vision (images), contexte de 262 144 jetons. Architecture hybride (une couche d'attention pleine sur quatre) : le cache de contexte reste petit.":
    "April 2026 generation: agentic, reliable tool calls, vision (images), 262,144-token context. Hybrid architecture (one full attention layer out of four): the context cache stays small.",
  "Génération de février 2026, multimodale (texte et images), contexte de 262 144 jetons, appels d'outils. Le 9B tient entièrement dans une carte de 6 Go en IQ4.":
    "February 2026 generation, multimodal (text and images), 262,144-token context, tool calls. The 9B fits entirely on a 6GB card in IQ4.",
  "Flagship multilingue (119+ langues), mode thinking hybride, dense et MoE.":
    "Multilingual flagship (119+ languages), hybrid thinking mode, dense and MoE.",
  "MoE agentic-coding 256k contexte.": "Agentic-coding MoE, 256k context.",
  "Raisonnement profond : maths, logique, problèmes complexes.":
    "Deep reasoning: maths, logic, complex problems.",
  "Spécialiste code et refactoring (0.5B à 32B).": "Code and refactoring specialist (0.5B to 32B).",
  "Raisonnement étape par étape (1.5B à 671B).": "Step-by-step reasoning (1.5B to 671B).",
  "MoE code (16B Lite & 236B Full).": "Code MoE (16B Lite & 236B Full).",
  "MoE multimodal (Scout 109B, Maverick 400B).": "Multimodal MoE (Scout 109B, Maverick 400B).",
  "Ultra-rapides et légers (1B, 3B).": "Ultra-fast and lightweight (1B, 3B).",
  "Modèle 7B iconique.": "Iconic 7B model.",
  "22B pour 80+ langages.": "22B for 80+ languages.",
  "14B raisonnement exceptionnel.": "14B with outstanding reasoning.",
  "SLM 3.8B edge-ready avec function calling.": "Edge-ready 3.8B SLM with function calling.",
  "9B conversationnel chinois/anglais 128k.": "9B Chinese/English conversational model, 128k.",
  "Famille MoE ultra-rapide pour raisonnement et agents autonomes.":
    "Ultra-fast MoE family for reasoning and autonomous agents.",
  "Hybrid MoE 30B (3.6B actifs) pour workflows agentiques 1M tokens.":
    "Hybrid 30B MoE (3.6B active) for 1M-token agentic workflows.",
  "Versions GGUF vérifiées de Qwen3 pour llama.cpp. Le 4B privilégie la vitesse, le 8B est le meilleur compromis sur un GPU 6 Go et le 14B utilise aussi la RAM pour gagner en qualité.":
    "Verified Qwen3 GGUF builds for llama.cpp. The 4B favours speed, the 8B is the sweet spot on a 6GB GPU, and the 14B also uses RAM for extra quality.",
  "Modèles multimodaux GGUF avec projecteur vision installé automatiquement. Le 4B tient confortablement sur une configuration 6 Go, le 12B fonctionne en mode hybride RAM + GPU.":
    "Multimodal GGUF models with the vision projector installed automatically. The 4B fits comfortably on a 6GB setup, the 12B runs in hybrid RAM + GPU mode.",
  "Qwen3.8 27B multimodal et raisonnant, proposé dans le format GGUF compatible avec le moteur local llama.cpp. Le projecteur vision adapté est installé automatiquement.":
    "Multimodal, reasoning Qwen3.8 27B in GGUF format compatible with the local llama.cpp engine. The matching vision projector is installed automatically.",
  "Raisonnement étape par étape dans un distillat Llama 70B prêt pour GPU haut de gamme. Format GGUF local.":
    "Step-by-step reasoning in a Llama 70B distillate ready for high-end GPUs. Local GGUF format.",
  "Le plus grand modèle de la famille Qwen2.5 Instruct, optimisé pour le dialogue et le travail long en GGUF local.":
    "The largest model in the Qwen2.5 Instruct family, optimised for dialogue and long-form work in local GGUF.",
};

export const EN_PATTERNS: Record<string, string> = {
  "Chargé : {}": "Loaded: {}",
  "WSL · {}": "WSL · {}",
  "Distribution par défaut · {}": "Default distribution · {}",
  "Distribution · {}": "Distribution · {}",
  "Exécuter ce code ({}) dans WSL ({}) ?": "Run this code ({}) in WSL ({})?",
  "Raison : {}": "Reason: {}",
  "Écriture de {}…": "Writing {}…",
  "Préparation de {}…": "Preparing {}…",
  "{} caractères": "{} characters",
  "{} k caractères": "{} k characters",
  "Ouvrir {} dans le panneau Fichiers": "Open {} in the Files panel",
  "Réunion dans « {} »": "Merged into “{}”",
  "Impossible de créer la conversation : {}": "Cannot create the conversation: {}",
  "La conversation n'a pas pu être créée, votre message n'a donc pas été envoyé. {}":
    "The conversation could not be created, so your message was not sent. {}",
  "Téléchargement : {}{}": "Download: {}{}",
  "Téléchargement de {}": "Downloading {}",
  "Téléchargement de {} — {} %": "Downloading {} — {} %",
  "Rechercher « {} »": "Search “{}”",
  "Laisse {} de VRAM libres sur {} ({} %)": "Keeps {} of VRAM free out of {} ({} %)",
  "Laisse {} de mémoire libres sur {}": "Keeps {} of memory free out of {}",
  "Garde {} des {} de VRAM libres pendant la génération, et {} des {} de RAM libres pour le système et vos applications":
    "Keeps {} of the {} of free VRAM while generating, and {} of the {} of free RAM for the system and your apps",
  "Garde {} des {} de mémoire libres pour le système et vos applications":
    "Keeps {} of the {} of free memory for the system and your apps",
  "Outils différés ({})": "Deferred tools ({})",
  "▶ Exécuter ({})": "▶ Run ({})",
  "{} jetons écrits à {} par seconde ; le prompt a été lu à {} par seconde. Mesuré par le moteur.":
    "{} tokens written at {} per second; the prompt was read at {} per second. Measured by the engine.",
  "Exécute {} {}…": "Running {} {}…",
  " ({} échecs)": " ({} failures)",
  "Serveur ({})": "Server ({})",
  "Serveur {}": "Server {}",
  "Ouvrir le dossier {}": "Open folder {}",
  "Modèle actif : {}": "Active model: {}",
  "Vérification de {} sur {}…": "Checking {} on {}…",
  "Installer {}": "Install {}",
  "{} est maintenant le modèle de la conversation.": "{} is now the conversation's model.",
  "Rechercher parmi {} modèles…": "Search among {} models…",
  "Retirer « {} » ?": "Remove “{}”?",
  "Certificat de « {} » installé.": "Certificate of “{}” installed.",
  "Extension {}": "Extension {}",
  "Installé et à jour ({})": "Installed and up to date ({})",
  "Installé ({}) — mise à jour {} disponible": "Installed ({}) — update {} available",
  "{} — une par ligne.": "{} — one per line.",
  "Réglages de {}": "Settings of {}",
  "Autorisations demandées par {}": "Permissions requested by {}",
  "Écran apporté par {}.": "Screen provided by {}.",
  "{} installée et activée — {}.": "{} installed and enabled — {}.",
  "{} installée, désactivée pour l'instant.": "{} installed, disabled for now.",
  "Permissions de {} enregistrées.": "Permissions of {} saved.",
  "{} mise à jour — v{}.": "{} updated — v{}.",
  "Désinstaller « {} » et supprimer ses fichiers ?": "Uninstall “{}” and delete its files?",
  "Tout mettre à jour{}": "Update all{}",
  "Tout ({})": "All ({})",
  "{} entrées": "{} entries",
  "v{} disponible sur la branche par défaut du dépôt":
    "v{} available on the repository's default branch",
  "Détail de {}": "Details of {}",
  "Installer v{}": "Install v{}",
  "Détection impossible : {}": "Detection failed: {}",
  "{} serveurs seront ajoutés.": "{} servers will be added.",
  "Outils de {}": "Tools of {}",
  "Autoriser l'outil {}": "Allow tool {}",
  "mis à jour le {}": "updated on {}",
  'Voulez-vous vraiment supprimer le modèle local "{}" pour libérer de l\'espace disque ?':
    'Do you really want to delete the local model "{}" to free disk space?',
  "Installer (({}))": "Install ({})",
  "Installer ({})": "Install ({})",
  "Installer la quantisation {}": "Install quantization {}",
  "Choisir la quantisation {}": "Choose quantization {}",
  "{} est ajouté. Choisissez son modèle ci-dessous.": "{} is added. Choose its model below.",
  "La conversation utilise maintenant {} sur {}.": "The conversation now uses {} on {}.",
  "Modèle de {}": "Model of {}",
  "Apporté par {}": "Provided by {}",
  "au-delà de {} s": "beyond {} s",
  "au-delà de {} min": "beyond {} min",
  "Encore {} s, à usage unique.": "{} s left, single-use.",
  "Profil indisponible : {}": "Profile unavailable: {}",
  "{} couches sur GPU, reste en RAM": "{} layers on GPU, the rest in RAM",
  "Auto — {} cœurs détectés": "Auto — {} cores detected",
  "Modèles servis par {}, facturés sur votre clé.": "Models served by {}, billed to your key.",
  "Voir les modèles {}": "See {} models",
  "{} modèles distants": "{} remote models",
  "Supprimer l'identifiant « {} » ?": "Delete login “{}”?",
  "Sélectionner « {} »": "Select “{}”",
  "Archiver « {} » ? Le projet disparaît de la liste. Ses conversations restent sur le disque.":
    "Archive “{}”? The project leaves the list. Its conversations stay on disk.",
  "Outil : {}": "Tool: {}",
  " · dernière mesure le {}": " · last measured on {}",
  "{} libérés.": "{} freed.",
  "Hors du dossier de stockage : {}": "Outside the storage folder: {}",
  "Tapez « {} » pour confirmer": "Type “{}” to confirm",
  "QR de connexion de {}": "Connection QR of {}",
  "{} fichier(s)": "{} file(s)",
  " · {}/{} étapes": " · {}/{} steps",
  " · {} outil(s)": " · {} tool(s)",
  "commande dans {}": "command in {}",
  "{} Go entièrement sur le GPU ({} Go libres), contexte de {} jetons. Environ {} jetons/s.":
    "{} GB entirely on the GPU ({} GB free), {}-token context. About {} tokens/s.",
  "{} couches sur {} tiennent dans les {} Go de VRAM libres, le reste passe par la RAM. Environ {} jetons/s.":
    "{} of {} layers fit in the {} GB of free VRAM, the rest goes through RAM. About {} tokens/s.",
  "{} répond sur {}.": "{} answers on {}.",
  "{} est installée mais ne répond pas. Démarrez-la depuis ce dossier.":
    "{} is installed but not answering. Start it from this folder.",
  "{} est installée.": "{} is installed.",
  "Enregistrer {} sous :": "Save {} as:",
  "Objectif non atteint après {} tour(s) : le modèle fait le compte rendu de ce qui a bloqué.":
    "Goal not reached after {} round(s): the model reports what blocked it.",
  "pas atteint en {} tours": "not reached in {} rounds",
  "Fichier vide ou illisible : {}": "Empty or unreadable file: {}",
  "L'outil « {} » ne s'exécutera pas sans votre réponse.":
    "The tool “{}” won't run without your answer.",
  "Enregistrement de la note vocale impossible : {}": "Cannot record the voice note: {}",
  "« {} » non joint : {}.": "“{}” not attached: {}.",
  "La conversation n'a pas pu être créée, votre message n'est pas parti : {}. Il est resté dans le champ.":
    "The conversation could not be created, your message wasn't sent: {}. It stayed in the field.",
  "Exécuter ce code ({}) sur votre machine ? {}": "Run this code ({}) on your machine? {}",
  "La commande /{} n'a produit aucun texte.": "The /{} command produced no text.",
  "Connecteur MCP · {} n'est pas ouvert": "MCP connector · {} is not open",
  "{} pièce(s) jointe(s)": "{} attachment(s)",
  "~{} jetons utilisés — maximum inconnu tant qu'aucun modèle n'est chargé (maintenir pour compresser)":
    "~{} tokens used — maximum unknown while no model is loaded (hold to compress)",
  "~{} / {} tokens utilisés — maintenir pour compresser": "~{} / {} tokens used — hold to compress",
  "Contexte : ~{} jetons utilisés, maximum inconnu tant qu'aucun modèle n'est chargé. Maintenir pour compresser la conversation.":
    "Context: ~{} tokens used, maximum unknown while no model is loaded. Hold to compress the conversation.",
  "Contexte : ~{} sur {} jetons. Maintenir pour compresser la conversation.":
    "Context: ~{} of {} tokens. Hold to compress the conversation.",
  "Archiver {} conversations ? Elles quittent la liste mais restent dans les archives.":
    "Archive {} conversations? They leave the list but stay in the archives.",
  "Actions sur {}": "Actions on {}",
  "Chargée dans le moteur : {}. ": "Loaded in the engine: {}. ",
  "Plafond du modèle : {}. ": "Model ceiling: {}. ",
  "température {}": "temperature {}",
  "pénalité {}": "penalty {}",
  "Réglages recommandés par {} : ": "Settings recommended by {}: ",
  "Dans {}": "In {}",
  "Fermer {}": "Close {}",
  " · archivée le {}": " · archived on {}",
  "Débridage activé pour « {} ». Le modèle répondra sans restriction ni censure.":
    "Unlock enabled for “{}”. The model will answer without restriction or censorship.",
  "Débridage retiré pour « {} » (Mode éthique standard rétabli).":
    "Unlock removed for “{}” (standard ethical mode restored).",
  "Supprimer « {} » ? {}": "Delete “{}”? {}",
  "{} extraits · {} source(s)": "{} chunks · {} source(s)",
  "Chargement de {}": "Loading {}",
  "éjection dans {}": "ejecting in {}",
  "Charger {}": "Load {}",
  "{} jetons/s": "{} tokens/s",
  "Cache {} jetons": "Cache {} tokens",
  "Trop lourd pour la VRAM au niveau « prudent » : {} Go à poser sur la carte, {} Go libres dont {} Go restent réservés au système. Le modèle déborderait sur la RAM.{}":
    "Too heavy for VRAM at the “safe” level: {} GB to put on the card, {} GB free of which {} GB stay reserved for the system. The model would spill into RAM.{}",
  "Garde {} de VRAM libres pour le système (carte de {})":
    "Keeps {} of VRAM free for the system ({} card)",
  "Trop de VRAM déjà utilisée pour charger ce modèle : {} Go à poser sur la carte, {} Go libres en ce moment, dont {} Go restent réservés au système par la prudence choisie. Fermez une application qui utilise la carte graphique (jeu, logiciel 3D…), ou choisissez une version plus légère.{}":
    "Too much VRAM already in use to load this model: {} GB to put on the card, {} GB free right now, of which {} GB stay reserved for the system by the chosen caution. Close an app using the graphics card (game, 3D software…), or pick a lighter version.{}",
  "Trop de mémoire déjà utilisée pour charger ce modèle : {} Go nécessaires, {} Go libres en ce moment, dont {} Go restent réservés au système par la prudence choisie. Fermez une application gourmande, ou choisissez une version plus légère.{}":
    "Too much memory already in use to load this model: {} GB needed, {} GB free right now, of which {} GB stay reserved for the system by the chosen caution. Close a memory-hungry app, or pick a lighter version.{}",
  "Fluide GPU — ~{}": "Smooth GPU — ~{}",
  "{} couches GPU — ~{}": "{} GPU layers — ~{}",
  "{} couches GPU": "{} GPU layers",
  "Sortie {}": "Released {}",
  "▼ {} variantes ({})": "▼ {} variants ({})",
  "Annuler ({}%)": "Cancel ({}%)",
  // La ligne repliée des appels d'outils (ToolRun) : l'état « en cours » dit
  // la recherche web quand le modèle est sur le net.
  "Exécute {} commande…": "Running {} command…",
  "Exécute {} commandes…": "Running {} commands…",
  "Exécute {} commande… et {} recherche web": "Running {} command… and {} web search",
  "Exécute {} commandes… et {} recherche web": "Running {} commands… and {} web search",
  "Exécute {} commande… et {} recherches web": "Running {} command… and {} web searches",
  "Exécute {} commandes… et {} recherches web": "Running {} commands… and {} web searches",
  "{} recherches web en cours…": "{} web searches in progress…",
  "Recherche web terminée ({} échecs)": "Web search finished ({} failures)",
  "{} recherches web terminées": "{} web searches finished",
  "{} recherches web terminées (1 échec)": "{} web searches finished (1 failure)",
  "{} recherches web terminées ({} échecs)": "{} web searches finished ({} failures)",
  "Exécuté {} commande": "Ran {} command",
  "Exécuté {} commandes": "Ran {} commands",
  "Exécuté {} commande (1 échec)": "Ran {} command (1 failure)",
  "Exécuté {} commandes (1 échec)": "Ran {} commands (1 failure)",
  "Exécuté {} commande ({} échecs)": "Ran {} command ({} failures)",
  "Exécuté {} commandes ({} échecs)": "Ran {} commands ({} failures)",
};
