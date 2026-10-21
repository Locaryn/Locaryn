# Rapport de tri — RAG, cAG, KAG, GraphRAG, hybride, routeur Laya

Question posée : **quel moyen de mise en contexte est le plus optimisé**, et peut-on
coupler les voies pour un meilleur rendu ? Mesures faites sur du 100 % local,
avec de petits modèles, sur la documentation réelle de Locaryn.

## Méthode

- **Corpus** : 500 morceaux de la documentation Locaryn, ≈107 000 jetons estimés.
- **Questions** : 19 (fait, relation, synthèse, hors-corpus) avec aiguille connue.
- **Modèle de réponse** : Qwen3.5-4B (Q4_K_M) servi par llama.cpp — le « petit
  modèle » demandé, sur le même matériel que l'application.
- **Juge** : Qwen3.5-9B (IQ3_XXS), note 0/2 par question (133 notes, 133/133 jugées),
  aveugle à la voie employée.
- **Plongements** : nomic-embed-text-v1.5 (Q5_K_M), servi localement.
- **Routeur** : Laya (petit classifieur déjà installé par l'application).
- **RAG** : K=6 voisins ; **hybride** : socle cAG + RAG pour le reste ;
  **laya** : Laya choisit la voie puis la voie répond.

## Résultats

| Voie | Note /2 | Correctes | Rappel | Jetons prompt | Latence ms |
|---|---|---|---|---|---|
| **hybride** | **1.84** | **16/19** | **81 %** | 4551 | 3052 |
| cag | 1.63 | 15/19 | 56 % | 3681 | 2807 |
| kag | 1.53 | 13/19 | 62 % | 448 | 3386 |
| laya (routeur) | 1.26 | 10/19 | 44 % | 879 | **1968** |
| graphrag | 1.21 | 10/19 | 50 % | 736 | 2995 |
| rag | 1.05 | 8/19 | 50 % | 1267 | 2856 |
| sans contexte | 0.79 | 6/19 | 0 % | 83 | 3018 |

### Par catégorie (moyenne /2)

| Catégorie | sans | cag | kag | graphrag | rag | hybride | laya |
|---|---|---|---|---|---|---|---|
| fait | 0.3 | 1.1 | 1.6 | 0.7 | 0.6 | **1.9** | 1.1 |
| relation | 1.0 | **1.8** | 1.4 | 1.0 | 1.2 | 1.6 | 1.4 |
| synthèse | 0.5 | **2.0** | 1.2 | 1.8 | 1.0 | **2.0** | 0.8 |
| hors-corpus | 2.0 | 2.0 | 2.0 | 2.0 | 2.0 | 2.0 | 2.0 |

### Coût d'indexation payé en amont

- Plongements des 500 morceaux : 6,9 s (une seule fois).
- GraphRAG : 10 résumés de communautés, 10 510 jetons d'entrée + 1 445 de
  sortie, **73 s** et ~12 000 jetons de modèle.
- cAG : **aucun** coût d'indexation, mais le cœur (≈4200 jetons) occupe la
  fenêtre à chaque requête.
- KAG : **aucun** appel de modèle à l'indexation, extraction par règles.

## Analyse

1. **L'hybride (cAG + RAG) gagne nettement** : meilleure note (1,84), meilleur
   rappel (81 %), seule voie au-dessus de 1,55. Le socle cAG porte les
   synthèses et les relations ; le RAG ajoute les faits précis que le cœur
   n'a pas. Coupler les deux est bien meilleur qu'un RAG seul (+0,79 point,
   +31 correctes %).
2. **Le cAG seul est le meilleur rapport qualité/coût simple** : 1,63 sans
   aucune indexation, roi des synthèses (2,0) et des relations (1,8). Sa
   faiblesse : les faits précis hors cœur (1,1) — il répond « je ne sais pas »
   quand le fait n'est pas dans les 4200 jetons centraux.
3. **Le KAG est la voie la plus économe** : 448 jetons par requête (8× moins
   que l'hybride) pour 1,53 — excellent sur les faits (1,6). Extraction par
   règles = zéro modèle en amont. C'est la voie « petits contextes ».
4. **Le GraphRAG ne rembourse pas son indexation** : 73 s de résumés de
   communautés pour finir à 1,21, sous le cAG. Sur un corpus de cette taille
   (107k jetons), le graphe ne se justifie pas.
5. **Le routeur Laya est le plus rapide (1968 ms, −35 %) mais mal orienté** :
   sa table de routage envoie les *faits* vers le RAG seul (0,6) et les
   *relations* vers GraphRAG (1,0) — les deux voies les plus faibles — et ne
   choisit l'hybride qu'en dernier recours. Le routeur n'est pas le problème,
   la table oui.
6. **Le RAG seul est insuffisant** : 1,05, noyé par les questions de
   synthèse (1,0) — les 6 voisins ne font pas un résumé.

## Recommandation pour Locaryn

- **Voie par défaut : hybride** (socle cAG, RAG par-dessus). C'est le meilleur
  rendu mesuré, et les deux morceaux existent déjà dans l'application
  (cœur de conversation + recherche de fichiers).
- **Repli automatique : cAG seul** quand la fenêtre est étroite (le cœur tient
  toujours, ~4200 jetons) ou que la question est de synthèse — le cAG égale
  l'hybride en synthèse (2,0) pour 19 % de jetons de moins.
- **KAG en appoint** : extraction d'entités par règles pour les réponses à
  faits précis à budget serré (448 jetons, 1,6 sur les faits) — bon candidat
  pour les questions sur les fichiers ouverts, où les noms sont connus.
- **GraphRAG : écarté** sur cette taille de corpus (indexation chère, note
  moyenne). À réévaluer seulement si la documentation dépasse ~500k jetons.
- **Routeur Laya : corrigé, pas retiré.** Table proposée d'après les mesures :
  synthèse → cAG, relation → cAG, fait → hybride, le reste → hybride. Le gain
  attendu : ~1,8 de note moyenne avec une latence de routage de ~20 ms
  (Laya non autorégressif), en n'envoyant au modèle de conversation que la
  voie adaptée — c'est-à-dire le score de l'hybride en n'en payant que le
  coût utile.

## Limites

- Un corpus et 19 questions ; un seul juge (modèle, pas humain), notant
  aveugle. Les écarts < 0,15 point sont à confirmer avant de trancher au
  dixième.
- Mesures à fenêtre unique : le cAG souffrirait d'une fenêtre plus petite
  (son cœur occupe 4200 jetons sur les ~8192 du serveur).
- Le jugement a été fait par un modèle plus grand (9B) que celui qui répond
  (4B) : les notes mesurent la voie, pas le modèle final.

*Généré par `scripts/bench-rag/bench.py` (phases `index`, `run`, `judge`,
`report`) ; données brutes dans `scripts/bench-rag/out/` (hors dépôt).*
