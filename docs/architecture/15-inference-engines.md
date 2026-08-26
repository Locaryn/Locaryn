# 15 — Moteurs d'inférence apportés par une extension

> Une extension peut apporter le programme qui **calcule les jetons**. Locaryn
> l'installe, le lance, le sonde et l'arrête comme son runtime intégré, sans
> qu'une ligne de son code ne nomme ce moteur. Le runtime intégré reste le
> défaut ; l'utilisateur choisit le moteur actif dans Réglages → Moteur.

---

## 1. Moteur ou noyau : deux choses différentes

| | Noyau (doc 14) | Moteur (ce document) |
| --- | --- | --- |
| Ce qui change | l'**agent** : sa boucle, sa mémoire, ses skills | **qui calcule les jetons** |
| Choisi par | une session (`sessions.core_id`) | le fournisseur actif (`providers.engine`) |
| Boucle d'outils, approbation, streaming | ceux du noyau | ceux de Locaryn, inchangés |
| Section du manifeste | `core` | `engine` |
| Pont | `packages/core-bridge` | `services/provider-supervisor` |

Un serveur d'inférence — llama-server, vLLM, SGLang, FreeToken — est un
**moteur**. Le confondre avec un noyau donnerait une session qui contourne la
boucle d'outils de Locaryn pour un programme qui ne fait que servir des poids.

---

## 2. Ce qui a changé dans le socle

### 2.1 Le moteur n'est plus une liste fermée

`ProviderEngine` porte une variante `Extension(String)`, dont le jeton est
`ext:<id>`. Ce jeton est la forme canonique : c'est lui qui est écrit en base,
transporté sur le fil SSE et accepté en argument de ligne de commande.

La sérialisation serde suit le jeton, si bien qu'un moteur d'extension reste une
**chaîne** sur le fil (`"ext:freetoken"`) et non un objet : les clients qui
lisent `engine` comme une chaîne continuent de fonctionner.

La table des jetons vit une seule fois, dans `shared-types`. Elle était recopiée
dans le stockage (deux fois), le daemon, la CLI et le superviseur — cinq copies
qui divergeaient : la CLI acceptait des moteurs que la base ne savait pas écrire.
`ProviderEngine::from_token` renvoie `None` sur un jeton inconnu, plutôt qu'un
moteur au hasard.

### 2.2 Le superviseur lance ce qu'on lui décrit

`services/provider-supervisor/src/extension_engine.rs` sait lancer un moteur
qu'il ne connaît pas : il lit une liste d'arguments, y substitue des chemins que
seul l'hôte connaît, lance le processus, attend la sonde, journalise la sortie
dans `engine-<id>.log`.

L'hôte — bureau ou daemon — remplit le registre depuis les extensions installées
(`set_extension_engines`) au démarrage puis à chaque changement. **Remplacer**
plutôt qu'ajouter est volontaire : une extension désactivée doit disparaître du
registre, et son processus est arrêté.

Rien dans ce module ne nomme un moteur. Ce qui est propre à un moteur — un
passage par WSL2, une conversion de checkpoint, un choix de backend — appartient
au programme que l'extension livre dans son `bin/`.

### 2.3 L'éligibilité d'un modèle dépend des moteurs installés

C'était le vrai verrou. `is_text_chat_model()` refusait tout ce qui n'était pas
`.gguf`, et `pull_hf_repo` refusait activement les checkpoints Transformers en
`safetensors`. Installer un moteur capable de les servir ne changeait rien :
l'écran des modèles continuait de les masquer.

La question a donc été coupée en deux :

- `is_non_chat_asset()` — **indépendant du moteur** : un vocodeur, un VAE ou un
  encodeur de plongements ne tient pas une conversation, quel que soit le
  programme qui le charge ;
- le **format**, qui dépend du moteur : `formats_des_moteurs()` agrège ce que les
  moteurs installés déclarent servir, et `is_chat_model_for()` répond en tenant
  compte des deux.

Un moteur qui déclare `model_formats.directories` fait apparaître les
**répertoires** de checkpoint comme modèles — c'est le nom du répertoire qui
devient le modèle, pas un de ses shards, parce que c'est le répertoire que le
moteur reçoit.

`load_chat_model` choisit ensuite le moteur qui sait charger le modèle demandé,
au lieu de supposer llama.cpp. Un modèle que personne ne sait charger renvoie une
erreur qui dit quoi installer.

### 2.4 Les chemins donnés aux extensions vivent à un seul endroit

`packages/extensions/src/hostpaths.rs` porte l'assainissement du nom d'extension
et les variables d'environnement génériques. Le bureau et le daemon les
calculaient chacun de leur côté, avec **deux assainissements différents** : une
extension dont le nom contient un tiret bas recevait deux dossiers privés selon
l'hôte qui l'avait lancée. Le serveur MCP et le moteur d'une même extension
partagent maintenant le même dossier d'état, par construction.

---

## 3. Ce qui ne change pas

- Le runtime intégré reste le défaut, et le seul moteur des installations qui
  n'ajoutent rien.
- L'agent compatible OpenAI, la boucle d'outils, l'approbation, le streaming, la
  persistance et les métriques : inchangés. Un moteur d'extension parle le même
  dialecte, il passe par le même chemin.
- Les migrations : aucune. La colonne `providers.engine` est du texte, et
  `ext:<id>` y tient sans schéma nouveau.

---

## 4. Sécurité

- **Loopback obligatoire.** `validate_engine` refuse une `api_url` qui n'est pas
  locale. Ce qui écoute ailleurs n'est pas un runtime supervisé : c'est un
  service distant, et l'accepter enverrait les conversations à une adresse posée
  dans un manifeste. La règle est partagée avec les noyaux
  (`manifest::is_loopback_url`).
- **Jamais de shell dans `lifecycle.start`** : une liste d'arguments, un
  environnement contrôlé.
- **Version épinglée** (`install.version`) pour la chaîne d'approvisionnement.
- **Permissions du manifeste** : un moteur exige `shell` — il lance un programme
  tiers avec les droits de l'utilisateur — et la fenêtre de permissions le dit.
- **Le journal est confiné** : `inference_engine_log` refuse un jeton qui ne
  correspond à aucun moteur enregistré, pour qu'un appel ne puisse pas nommer un
  fichier arbitraire.

---

## 5. Reste ouvert

- **Un seul dialecte** : `openai_compat`. Un moteur qui ne parlerait que
  l'API Anthropic (`/v1/messages`) demanderait un second pilote ; aucun besoin
  réel pour l'instant, les moteurs connus servent les deux.
- **Mode distant** : les moteurs d'extension sont locaux. En mode distant, c'est
  le moteur du serveur qui répond ; l'écran ne propose pas d'en installer un.
- **Plusieurs moteurs actifs à la fois** : un seul fournisseur est actif, comme
  aujourd'hui. Deux moteurs chargés en même temps se disputeraient la VRAM sans
  que rien n'arbitre.
- **Modèles compagnons** : un moteur qui exigerait un fichier compagnon
  (projecteur, encodeur) le déclare aujourd'hui dans `downloads` de son
  catalogue, comme les autres extensions ; rien ne le vérifie côté moteur.

---

## 6. Extension de référence

[`plugin-freetoken`](https://github.com/Locaryn/plugin-freetoken) met tout cela
en œuvre : section `engine` complète, lanceur qui passe par WSL2 sous Windows,
catalogue de checkpoints Mixture-of-Experts vérifié en CI, outils MCP de
diagnostic, compétence qui explique au modèle quel outil répond à quelle
question.
