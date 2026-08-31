# 16 — Est-ce que ce modèle tourne sur cette machine

> `packages/llmfit` répond à la seule question que se pose l'utilisateur devant
> une liste de modèles : **est-ce que ça tourne chez moi, et à quelle vitesse**.
> Tout est natif — aucun binaire externe, aucun service, aucun appel réseau.

---

## 1. Pourquoi un calcul, et pas un pourcentage

L'application répondait par une règle de trois : taille du fichier × 1,15, comparée
à la mémoire libre. Cette règle se trompe dans les deux sens.

Elle refuse ce qui passerait. Un modèle de 12 Go sur une carte de 6 Go était
déclaré « trop lourd » alors que la moitié de ses couches tient sur le GPU et
que le reste tourne en RAM — lentement, mais il tourne.

Elle accepte ce qui déborde. Le cache d'attention ne pèse pas 15 % des poids :
il dépend du contexte demandé et du nombre de têtes de clé. À 32 768 jetons, un
modèle de 12B en Q2 demande 5 Go de cache pour 4,4 Go de poids. Annoncer
« confortable » sur la seule taille du fichier, c'est promettre un chargement
qui échouera.

---

## 2. Les trois postes de mémoire

| Poste | D'où il vient | Ce qui le corrige |
| --- | --- | --- |
| **Poids** | somme des tenseurs déclarés dans l'en-tête GGUF | une quantification plus basse |
| **Cache d'attention** | `2 × couches × contexte × têtes_kv × dim_tête × précision` | un contexte plus court, ou un cache en `q8_0` / `q4_0` |
| **Tampons de calcul** | activations, masque, logits — et le produit clés-requêtes quand l'attention éclair est désactivée | activer l'attention éclair |

Les séparer n'est pas cosmétique : ils ne se corrigent pas de la même façon, et
l'interface montre les trois pour que l'utilisateur sache lequel réduire.

---

## 3. Lecture de l'en-tête GGUF

`gguf.rs` lit l'en-tête et rien d'autre. Les tenseurs sont décrits par leurs
dimensions, jamais parcourus : un fichier de 40 Go se résume en quelques
millisecondes. Le tableau du vocabulaire est sauté sans être matérialisé — seule
sa longueur compte.

Ce qui en sort : architecture, nombre de couches, dimension d'embedding, têtes
de clé, dimension de tête, vocabulaire, contexte d'entraînement, nombre
d'experts, octets par bloc transformeur, et le type ggml majoritaire — la
quantification effective.

Un format dont l'en-tête n'est pas lisible (safetensors, checkpoint PyTorch)
retombe sur la taille sur disque, et le rapport le signale au lieu de faire
passer une déduction pour une mesure.

---

## 4. La machine, mesurée

`hardware.rs` sonde une fois par session : RAM totale, GPU et sa mémoire,
cœurs. La mémoire **libre** est relue régulièrement, avec cinq secondes de
validité — sans ce cache, estimer trois cents lignes de catalogue relancerait
une sonde système par ligne.

La bande passante mémoire est **mesurée**, pas supposée : plusieurs fils lisent
un tampon plus grand que le dernier niveau de cache, et le meilleur de trois
passes est retenu. C'est ce que fait l'inférence à chaque jeton. Celle de la
carte graphique vient d'une table des modèles courants, avec un repli
proportionnel à la mémoire embarquée.

---

## 5. Le verdict

1. Les trois postes sont additionnés pour le contexte réglé.
2. Si tout tient dans la VRAM libre, réserve déduite : **confortable**.
3. Sinon, on compte combien de couches y tiennent, cache compris, et le reste
   passe en RAM : **juste**, avec la répartition affichée.
4. Ce qui ne tient nulle part est **refusé**, sauf au niveau de prudence
   « risqué » qui prévient et laisse faire.

Le niveau de prudence ne multiplie plus la taille du fichier : il décide de la
réserve laissée libre à côté du calcul.

---

## 6. La vitesse

Générer un jeton oblige à relire tous les poids actifs, une fois. Le débit est
donc borné par la bande passante, pas par le calcul :

```
temps par jeton = octets_sur_gpu / bande_passante_vram
                + octets_en_ram  / bande_passante_ram
```

Les deux temps s'**additionnent** : un modèle réparti paie les deux, l'un après
l'autre. Un modèle à experts ne traverse qu'une partie de ses poids par jeton,
ce qui change la vitesse sans rien changer à la mémoire — tous les experts
doivent rester résidents.

La lecture du prompt, elle, est bornée par le calcul. C'est le chiffre le moins
sûr du rapport, et le rapport le dit.

---

## 7. Chaque chiffre porte ses hypothèses

Un débit annoncé sans ses conditions n'est pas vérifiable. Chaque rapport
transporte donc la liste de ce qu'il suppose : dimensions lues ou déduites,
précision et taille du cache, attention éclair, bande passante mesurée ou
supposée, répartition des couches. L'interface les montre repliées sous le
verdict.

---

## 8. Où c'est branché

| Endroit | Ce qu'il appelle |
| --- | --- |
| Garde-fou de chargement (`model_residency.rs`) | `llmfit::for_file` sur le fichier réel, avec les réglages d'inférence en vigueur |
| Liste des modèles (`ModelBrowser.tsx`) | `llmfit_catalog` : un seul appel dédoublonné pour toutes les fiches visibles, avant tout téléchargement |
| Écran « votre PC » (`check_hardware`) | le même sondage matériel, pour que les deux écrans ne puissent plus se contredire |

En ligne de commande, pour vérifier une estimation sur une vraie machine :

```bash
cargo run -p locaryn-llmfit --example rapport -- chemin/vers/modele.gguf 32768
```
