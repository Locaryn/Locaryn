#!/usr/bin/env python3
"""Banc de comparaison : RAG, cAG, KAG, GraphRAG et leur couplage, sur petits modèles.

Le corpus est la documentation même de Locaryn (docs/, contexte.md, README.md) :
c'est le genre de matière qu'un agent doit connaître, et elle est assez vaste
(~330 ko, ≈ 110 000 jetons) pour qu'aucune fenêtre locale ne puisse la porter en
entier. C'est exactement ce qui départage les moyens.

Sept conditions sont mesurées sur les mêmes questions :

  sans      aucun contexte — le plancher, pour voir ce que le corpus apporte.
  cag       un « cœur » stable placé en entier dans le contexte (cache-augmented).
  rag       les k morceaux les plus proches par vecteur dense.
  kag       des faits explicites (sujets, définitions, décisions) + leurs sources.
  graphrag  des communautés d'entités résumées une fois, puis parcourues.
  hybride   le cœur cAG pour le socle, le RAG pour le reste.
  laya      Laya (petit classifieur déjà installé par l'application) choisit,
            pour chaque question, entre le cAG, le RAG et le GraphRAG.

Trois familles de mesures sortent de chaque appel : le rappel de récupération
(l'aiguille — la chaîne sans laquelle la réponse est impossible — est-elle
réellement dans le contexte donné ?), la qualité de la réponse (jugée à l'aveugle
par un modèle plus gros), et le coût (jetons de prompt, latence, jetons payés à
l'indexation).

Phases :
    python bench.py index   --base http://127.0.0.1:8099   # indexe et construit
    python bench.py run     --base http://127.0.0.1:8099   # pose les questions
    python bench.py judge   --base http://127.0.0.1:8100   # juge à l'aveugle (autre modèle)
    python bench.py report                                # tableau final

`index` et `run` parlent au petit modèle, `judge` au grand : ils ne tournent donc
jamais en même temps (6 Go de mémoire vidéo ne portent pas deux modèles).
"""

from __future__ import annotations

import argparse
import json
import math
import os
import pathlib
import random
import re
import statistics
import sys
import time
import urllib.error
import urllib.request
from collections import defaultdict

# ── Où vivent les choses ────────────────────────────────────────────────────

RACINE = pathlib.Path(__file__).resolve().parent
DOSSIER = RACINE / "out"
CORPUS_FICHIERS = [
    "contexte.md",
    "README.md",
    "docs/extensions-interop.md",
    "docs/writing-an-extension.md",
    "docs/adr/ADR-0001-rust-tauri-core.md",
    "docs/adr/ADR-0002-http-sse-transport.md",
    "docs/adr/ADR-0003-open-core-license.md",
    "docs/adr/ADR-0004-mcp-compatibility.md",
    "docs/adr/ADR-0005-sqlite-persistence.md",
    "docs/adr/README.md",
    "docs/api/locaryn-deep-links.md",
    "docs/api/openai-compat.md",
    "docs/architecture/01-product-spec.md",
    "docs/architecture/02-architecture.md",
    "docs/architecture/03-tech-decisions.md",
    "docs/architecture/04-monorepo-plan.md",
    "docs/architecture/05-bootstrap.md",
    "docs/architecture/06-api-contract.md",
    "docs/architecture/07-persistence.md",
    "docs/architecture/08-ux.md",
    "docs/architecture/09-extension-model.md",
    "docs/architecture/10-roadmap.md",
    "docs/architecture/11-ssh-connector-plan.md",
    "docs/architecture/12-server-and-deployment.md",
    "docs/architecture/13-web-et-pwa.md",
    "docs/architecture/14-alternate-cores.md",
    "docs/architecture/15-inference-engines.md",
    "docs/architecture/16-estimation-de-charge.md",
    "docs/architecture/17-fournisseurs-distants.md",
]
RACINE_PROJET = RACINE.parent.parent  # scripts/bench-rag → racine du dépôt

# ── Réglages de l'expérience ────────────────────────────────────────────────

MORCEAU_CIBLE = 1100          # octets visés par morceau (≈ 350 jetons)
MORCEAU_CHEVAUCHE = 140
K_RAG = 6                     # morceaux remontés par le RAG dense
K_CAG = 4200                  # jetons du cœur cAG (≈ 14 ko)
TEMPERATURE = 0.0
MAX_JETONS = 420              # assez pour une réponse courte, pas pour divaguer
SEED = 20261020

SYSTEME = (
    "Tu es l'assistant technique du projet Locaryn, dont la documentation te "
    "fournie sert de seule source. Réponds en français, en trois phrases au "
    "plus, sans liste ni titre. Si le contexte ne contient pas la réponse, "
    "réponds exactement : je ne sais pas."
)

SYSTEME_SANS = (
    "Tu es l'assistant technique du projet Locaryn. Réponds en français, en "
    "trois phrases au plus. Si tu ne connais pas la réponse, réponds "
    "exactement : je ne sais pas."
)

ARRET = ["le", "la", "les", "de", "des", "du", "un", "une", "et", "ou", "en",
         "pour", "avec", "dans", "sur", "que", "qui", "quoi", "est", "sont",
         "quel", "quelle", "quels", "quelles", "comment", "pourquoi", "peux",
         "peut", "tout", "tous", "toutes", "leur", "leurs", "son", "ses",
         "this", "that", "the", "and", "for", "with", "what", "how", "locaryn"]


# ── HTTP ────────────────────────────────────────────────────────────────────

def _post(url: str, corps: dict, delai: int = 600) -> dict:
    """POST JSON, avec deux reprises : un serveur qui charge n'est pas une erreur."""
    donnees = json.dumps(corps, ensure_ascii=False).encode("utf-8")
    derniere: Exception | None = None
    for tentative in range(3):
        try:
            requete = urllib.request.Request(
                url, data=donnees, headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(requete, timeout=delai) as rep:
                return json.loads(rep.read().decode("utf-8"))
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as e:
            derniere = e
            time.sleep(2 * (tentative + 1))
    raise RuntimeError(f"POST {url} : {derniere}")


def plonger(base: str, textes: list[str]) -> list[list[float]]:
    """Les vecteurs d'une série de textes, par paquets."""
    vecteurs: list[list[float]] = []
    for debut in range(0, len(textes), 16):
        paquet = textes[debut:debut + 16]
        rep = _post(f"{base}/v1/embeddings",
                    {"input": paquet, "model": "bench"})
        vecteurs.extend(d["embedding"] for d in sorted(rep["data"], key=lambda d: d["index"]))
    return vecteurs


def plonger1(base: str, texte: str) -> list[float]:
    return plonger(base, [texte])[0]


def repondre(base: str, contexte: str | None, question: str) -> dict:
    """Une réponse, avec ce qu'elle coûte. Renvoie texte, jetons et latence."""
    if contexte:
        systeme = SYSTEME
        utilisateur = f"[CONTEXTE]\n{contexte}\n[FIN DU CONTEXTE]\n\nQuestion : {question}"
    else:
        systeme = SYSTEME_SANS
        utilisateur = f"Question : {question}"
    debut = time.perf_counter()
    rep = _post(f"{base}/v1/chat/completions", {
        "model": "bench",
        "messages": [{"role": "system", "content": systeme},
                     {"role": "user", "content": utilisateur}],
        "temperature": TEMPERATURE,
        "max_tokens": MAX_JETONS,
        "chat_template_kwargs": {"enable_thinking": False},
    })
    latence = (time.perf_counter() - debut) * 1000
    message = rep["choices"][0]["message"]
    usage = rep.get("usage", {})
    return {
        "texte": (message.get("content") or "").strip(),
        "jetons_prompt": usage.get("prompt_tokens", 0),
        "jetons_reponse": usage.get("completion_tokens", 0),
        "jetons_cache": usage.get("prompt_tokens_details", {}).get("cached_tokens", 0),
        "latence_ms": round(latence),
    }


# ── Corpus et morceaux ──────────────────────────────────────────────────────

def charger_corpus() -> list[dict]:
    documents = []
    for rel in CORPUS_FICHIERS:
        chemin = RACINE_PROJET / rel
        if not chemin.exists():
            print(f"  ! absent : {rel}")
            continue
        documents.append({"source": rel, "texte": chemin.read_text(encoding="utf-8", errors="replace")})
    return documents


def decouper(texte: str) -> list[tuple[str, str]]:
    """Découpe selon les titres markdown, puis rogne les morceaux trop longs."""
    sections: list[tuple[str, str]] = []
    titre, courant = "", []
    for ligne in texte.splitlines():
        if re.match(r"^#{1,4}\s", ligne):
            if courant:
                sections.append((titre, "\n".join(courant).strip()))
            titre, courant = ligne.lstrip("# ").strip(), [ligne]
        else:
            courant.append(ligne)
    if courant:
        sections.append((titre, "\n".join(courant).strip()))

    morceaux: list[tuple[str, str]] = []
    for titre, corps in sections:
        corps = corps.strip()
        if not corps:
            continue
        if len(corps) <= MORCEAU_CIBLE * 1.6:
            morceaux.append((titre, corps))
            continue
        pas = MORCEAU_CIBLE - MORCEAU_CHEVAUCHE
        for debut in range(0, len(corps), pas):
            bout = corps[debut:debut + MORCEAU_CIBLE]
            if len(bout) < 120 and morceaux:
                break
            morceaux.append((titre, bout))
    return morceaux


# ── KAG : des faits explicites ──────────────────────────────────────────────

DEFINITION = re.compile(r"^\s*(?:[-*+]\s+|\d+\.\s+)?[*_]{0,2}([A-Za-zÀ-ÿ][\w .,'’/()-]{2,45})[*_]{0,2}\s*[:—–]\s{1,2}([A-ZÀ-Ý0-9«\"].{15,})")
DECISION = re.compile(r"^\s*(?:[-*+]\s+)?\*\*([^*]{3,60})\*\*\s*[:—-]?\s*(.{10,})")


def extraire_faits(source: str, titre: str, corps: str) -> list[dict]:
    """Ce que le document *énonce* : définitions, décisions notées, lignes de tableau.

    Rien n'est déduit ni résumé à l'indexation : c'est la différence de fond avec
    le GraphRAG, dont les résumés sont écrits par un modèle.
    """
    faits = []
    if titre:
        premiere = next((l for l in corps.splitlines() if len(l.strip()) > 40 and not l.startswith("#")), "")
        if premiere:
            faits.append({"sujet": titre, "texte": premiere.strip()[:420], "source": source})
    for ligne in corps.splitlines():
        if ligne.strip().startswith("|") and ligne.count("|") >= 3:
            cellules = [c.strip() for c in ligne.strip().strip("|").split("|")]
            if any(cellules) and not all(set(c) <= set("-: ") for c in cellules):
                faits.append({"sujet": cellules[0][:60] or titre,
                              "texte": " | ".join(c for c in cellules if c)[:420],
                              "source": source})
            continue
        m = DECISION.match(ligne) or DEFINITION.match(ligne)
        if m:
            sujet, texte = m.group(1).strip(), m.group(2).strip()
            if len(sujet) >= 3 and len(texte) >= 15:
                faits.append({"sujet": sujet, "texte": f"{sujet} : {texte}"[:420], "source": source})
    return faits


def mots(texte: str) -> set[str]:
    return {m for m in re.findall(r"[\wÀ-ÿ-]{3,}", texte.lower()) if m not in ARRET}


def score_lexical(question: str, cible: str) -> int:
    return len(mots(question) & mots(cible))


# ── GraphRAG : entités, communautés, résumés ────────────────────────────────

IDENTIFIANT = re.compile(r"`([^`\n]{3,60})`|\*\*([A-Za-zÀ-ÿ][\w .,'’/-]{2,50})\*\*|"
                         r"\b([a-z]+(?:[_-][a-z]+){1,4}|[A-Z][a-z]+(?:[A-Z][a-z]+)+)\b")

IGNORER = {"locaryn", "true", "false", "none", "null", "http", "json", "todo"}


def entites(texte: str) -> list[str]:
    trouves = set()
    for m in IDENTIFIANT.finditer(texte):
        nom = next((g for g in m.groups() if g), None)
        if not nom:
            continue
        nom = nom.strip().strip("`*")
        if 3 <= len(nom) <= 60 and nom.lower() not in IGNORER:
            trouves.add(nom)
    return sorted(trouves)


def graphe_des_morceaux(morceaux: list[dict]) -> dict[str, dict]:
    """Cooccurrence d'entités dans un même morceau : l'arête dit « ces deux notions
    se traitent ensemble », et c'est tout ce qu'on lui demande."""
    voisins: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    attaches: dict[str, list[int]] = defaultdict(list)
    for i, m in enumerate(morceaux):
        noms = entites(m["texte"])[:24]
        m["entites"] = noms
        for nom in noms:
            attaches[nom].append(i)
            for autre in noms:
                if autre != nom:
                    voisins[nom][autre] += 1
    return {n: {"voisins": dict(v), "morceaux": attaches[n]} for n, v in voisins.items()}


def communautes(graphe: dict[str, dict]) -> list[list[str]]:
    """Propagation d'étiquettes, déterministe : pas de dépendance externe, et le
    résultat se relit (un module, un concept, un pilier)."""
    etiquette = {n: n for n in graphe}
    for _ in range(8):
        changement = 0
        for n in sorted(graphe):
            votes: dict[str, int] = defaultdict(int)
            for voisin, poids in graphe[n]["voisins"].items():
                votes[etiquette.get(voisin, voisin)] += poids
            if votes:
                meilleur = max(sorted(votes), key=lambda k: votes[k])
                if meilleur != etiquette[n]:
                    etiquette[n] = meilleur
                    changement += 1
        if not changement:
            break
    groupes: dict[str, list[str]] = defaultdict(list)
    for n, e in etiquette.items():
        groupes[e].append(n)
    out = sorted((sorted(v) for v in groupes.values()), key=len, reverse=True)
    return out


# ── Conditions ──────────────────────────────────────────────────────────────

def indexation(base: str, plongeur: str | None = None) -> dict:
    plongeur = plongeur or base
    print("Chargement du corpus…")
    documents = charger_corpus()
    total = sum(len(d["texte"]) for d in documents)
    print(f"  {len(documents)} documents, {total} octets ≈ {total // 3} jetons")

    morceaux: list[dict] = []
    for d in documents:
        for titre, corps in decouper(d["texte"]):
            if len(corps) < 60:
                continue
            morceaux.append({"source": d["source"], "titre": titre, "texte": corps})
    print(f"  {len(morceaux)} morceaux — plongements…")
    debut = time.perf_counter()
    vecteurs = plonger(plongeur, [f"{m['titre']}\n{m['texte']}" for m in morceaux])
    cout_plongement = time.perf_counter() - debut
    for m, v in zip(morceaux, vecteurs):
        m["vecteur"] = v
    print(f"  plongés en {cout_plongement:.1f}s, dim {len(vecteurs[0])}")

    print("Faits (KAG)…")
    faits = []
    for d in documents:
        for titre, corps in decouper(d["texte"]):
            faits.extend(extraire_faits(d["source"], titre, corps))
    print(f"  {len(faits)} faits")

    print("Graphe (GraphRAG)…")
    graphe = graphe_des_morceaux(morceaux)
    comm = communautes(graphe)
    print(f"  {len(graphe)} entités, {len(comm)} communautés (10 premières : "
          f"{[c[:3] for c in comm[:5]]})")

    # Résumés de communautés : la seule étape payée par un modèle à l'indexation,
    # et le vrai coût de différence du GraphRAG. On résume les dix plus grandes.
    cout_index = {"jetons_prompt": 0, "jetons_reponse": 0, "appels": 0, "secondes": 0.0}
    resumes: list[dict] = []
    for groupe in comm[:10]:
        extraits = []
        for nom in groupe[:14]:
            for i in graphe[nom]["morceaux"][:1]:
                extraits.append(morceaux[i]["texte"][:500])
        texte = "\n---\n".join(dict.fromkeys(extraits))[:6000]
        question = ("Résume en trois phrases, à partir du texte seul, ce que "
                    f"cela dit sur : {', '.join(groupe[:12])}.")
        debut = time.perf_counter()
        rep = _post(f"{base}/v1/chat/completions", {
            "model": "bench",
            "messages": [{"role": "system", "content": SYSTEME},
                         {"role": "user", "content": f"[CONTEXTE]\n{texte}\n[FIN]\n\n{question}"}],
            "temperature": 0.0, "max_tokens": 220,
            "chat_template_kwargs": {"enable_thinking": False},
        })
        cout_index["secondes"] += time.perf_counter() - debut
        cout_index["appels"] += 1
        cout_index["jetons_prompt"] += rep.get("usage", {}).get("prompt_tokens", 0)
        cout_index["jetons_reponse"] += rep.get("usage", {}).get("completion_tokens", 0)
        resumes.append({"entites": groupe[:20], "resume": rep["choices"][0]["message"]["content"]})
    print(f"  {cout_index['appels']} résumés écrits : {cout_index['jetons_prompt']} jetons "
          f"d'entrée, {cout_index['jetons_reponse']} de sortie, {cout_index['secondes']:.0f}s")

    # Le cœur cAG : des documents entiers, choisis à l'avance, pas selon la question.
    coeur, cibles = [], K_CAG
    for rel in ["contexte.md", "docs/architecture/02-architecture.md",
                "docs/architecture/03-tech-decisions.md", "docs/adr/README.md"]:
        doc = next((d for d in documents if d["source"] == rel), None)
        if not doc:
            continue
        cout = len(doc["texte"]) / 3
        if cout > cibles:
            coupure = int(cibles * 3 * 0.97)
            doc = {"source": rel, "texte": doc["texte"][:coupure]}
            coeur.append(doc)
            break
        coeur.append(doc)
        cibles -= cout
    coeur_texte = "\n\n".join(f"# {d['source']}\n{d['texte']}" for d in coeur)
    print(f"Cœur cAG : {len(coeur)} documents, ≈{len(coeur_texte)//3} jetons")

    return {"morceaux": morceaux, "faits": faits, "graphe": graphe,
            "communautes": comm[:10], "resumes": resumes,
            "coeur": [d["source"] for d in coeur], "coeur_texte": coeur_texte,
            "cout_index": cout_index, "cout_plongement_s": round(cout_plongement, 1)}


def contexte_rag(base: str, index: dict, question: str, k: int = K_RAG,
                 hors: set[str] | None = None, plongeur: str | None = None) -> str:
    """Les k morceaux les plus proches de la question (cosinus, en mémoire)."""
    q = plonger1(plongeur or base, question)
    normeq = math.sqrt(sum(x * x for x in q)) or 1.0
    scores = []
    for i, m in enumerate(index["morceaux"]):
        if hors and m["source"] in hors:
            continue
        v = m["vecteur"]
        normv = math.sqrt(sum(x * x for x in v)) or 1.0
        scores.append((sum(a * b for a, b in zip(q, v)) / (normeq * normv), i))
    scores.sort(reverse=True)
    return "\n---\n".join(
        f"[{index['morceaux'][i]['source']} — {index['morceaux'][i]['titre']}]\n{index['morceaux'][i]['texte']}"
        for _, i in scores[:k])


def contexte_kag(index: dict, question: str, k_faits: int = 6) -> str:
    """Des faits explicites, classés par recouvrement lexical avec la question."""
    classes = sorted(index["faits"], key=lambda f: score_lexical(question, f["sujet"] + " " + f["texte"]),
                     reverse=True)
    gardes = [f for f in classes if score_lexical(question, f["sujet"] + " " + f["texte"]) > 0][:k_faits]
    if not gardes:
        gardes = classes[:3]
    lignes = [f"[{f['source']}] {f['texte']}" for f in gardes]
    sources = {f["source"] for f in gardes}
    return "\n---\n".join(lignes) + "\n\nRepères d'origine : " + ", ".join(sorted(sources))


def contexte_graphrag(index: dict, question: str) -> str:
    """Graines tirées de la question → communauté → résumé + morceaux voisins."""
    q = mots(question)
    graines = sorted((n for n in index["graphe"] if q & mots(n)),
                     key=lambda n: len(q & mots(n)), reverse=True)[:6]
    vus: set[str] = set()
    groupes = []
    for g in index["resumes"]:
        touche = sum(1 for n in g["entites"] if n in graines or q & mots(n))
        if touche:
            groupes.append((touche, g))
    groupes.sort(key=lambda t: t[0], reverse=True)
    morceaux = []
    for n in graines[:3]:
        for i in index["graphe"][n]["morceaux"][:2]:
            if i not in vus:
                vus.add(i)
                morceaux.append(index["morceaux"][i])
    parties = []
    for _, g in groupes[:3]:
        parties.append(f"[communauté : {', '.join(g['entites'][:8])}]\n{g['resume']}")
    for m in morceaux[:3]:
        parties.append(f"[{m['source']} — {m['titre']}]\n{m['texte'][:700]}")
    return "\n---\n".join(parties) if parties else ""


# ── Laya, le routeur ────────────────────────────────────────────────────────

QUESTIONS_LAYA = {
    "relation": {"type": "noul",
                 "instructions": "Cette demande relie-t-elle plusieurs documents, modules ou notions differents ?"},
    "fait": {"type": "noul",
             "instructions": "Cette demande vise-t-elle un fait precis, une valeur, un nom ou une definition ?"},
    "synthese": {"type": "noul",
                 "instructions": "Cette demande demande-t-elle un apercu general de l'ensemble du sujet ?"},
}


def charger_laya():
    """Laya est déjà installé par l'application (packages/config → python-env)."""
    env = os.environ.copy()
    env.setdefault("HF_HOME", r"D:\Nouveau dossier (2)\hf_cache")
    import laya  # noqa: PLC0415 — import tardif : Laya pèse 46 s au chargement
    return laya.load("convaiinnovations/laya", subfolder="multilingual", device="cpu")


def sens_noul(agent) -> bool:
    """Laya renvoie une probabilité sans dire de quel côté est le « vrai » : deux
    phrases sentinelles, l'une franchement vraie, l'autre franchement fausse,
    fixent le sens une fois pour toutes."""
    vrai = agent.predict("Ce texte parle explicitement de configuration et de serveur.", QUESTIONS_LAYA)
    faux = agent.predict("Ce texte parle uniquement de recettes de cuisine et de jardinage.", QUESTIONS_LAYA)
    pv = vrai["answers"]["relation"]["noul"]
    pf = faux["answers"]["relation"]["noul"]
    return pv > pf  # True : noul est la probabilité du « vrai »


def router_laya(agent, sens: bool, question: str) -> tuple[str, dict]:
    debut = time.perf_counter()
    rep = agent.predict(question, QUESTIONS_LAYA)
    latence = (time.perf_counter() - debut) * 1000
    rep = rep["answers"]
    def proba(nom):
        p = rep[nom]["noul"]
        return p if sens else 1.0 - p
    p = {k: proba(k) for k in QUESTIONS_LAYA}
    if p["synthese"] > 0.5:
        voie = "cag"
    elif p["relation"] > 0.5:
        voie = "graphrag"
    elif p["fait"] > 0.5:
        voie = "rag"
    else:
        voie = "hybride"
    return voie, {"probabilites": {k: round(v, 4) for k, v in p.items()},
                  "latence_ms": round(latence)}


# ── Phase de mesure ─────────────────────────────────────────────────────────

def phase_run(base: str, index: dict, questions: list[dict], only: str | None,
              conds: list[str] | None = None, plongeur: str | None = None) -> list[dict]:
    toutes = ["sans", "cag", "rag", "kag", "graphrag", "hybride", "laya"]
    conditions = conds[:] if conds else toutes[:]
    agent_laya, sens = None, True
    if "laya" in conditions:
        try:
            agent_laya = charger_laya()
            sens = sens_noul(agent_laya)
            print(f"Laya prêt — sens du « noul » : {'probabilité du vrai' if sens else 'probabilité du faux'}")
        except Exception as e:  # noqa: BLE001 — Laya est un bonus, pas une condition vitale
            print(f"Laya indisponible ({e}) : la condition « laya » sera sautée.")
            conditions.remove("laya")

    resultats = []
    for q in questions:
        if only and q["id"] != only:
            continue
        print(f"\n── {q['id']} [{q['categorie']}] {q['question'][:70]}")
        for cond in conditions:
            contexte, note = None, None
            debut_c = time.perf_counter()
            if cond == "cag":
                contexte = index["coeur_texte"]
            elif cond == "rag":
                contexte = contexte_rag(base, index, q["question"], plongeur=plongeur)
            elif cond == "kag":
                contexte = contexte_kag(index, q["question"])
            elif cond == "graphrag":
                contexte = contexte_graphrag(index, q["question"])
            elif cond == "hybride":
                contexte = (index["coeur_texte"] + "\n\n"
                            + contexte_rag(base, index, q["question"], k=4,
                                           hors=set(index["coeur"]), plongeur=plongeur))
            elif cond == "laya":
                voie, note = router_laya(agent_laya, sens, q["question"])
                contexte = {"cag": index["coeur_texte"],
                            "rag": lambda: contexte_rag(base, index, q["question"], plongeur=plongeur),
                            "graphrag": lambda: contexte_graphrag(index, q["question"]),
                            "hybride": lambda: contexte_rag(base, index, q["question"], k=4,
                                                            plongeur=plongeur)}[voie]
                if callable(contexte):
                    contexte = contexte()
            cout = (time.perf_counter() - debut_c) * 1000

            rep = repondre(base, contexte or None, q["question"])
            aiguille = q.get("needle")
            rappel = (aiguille.lower() in (contexte or "").lower()) if aiguille else None
            ligne = {"question": q["id"], "categorie": q["categorie"], "condition": cond,
                     "reponse": rep["texte"], "jetons_prompt": rep["jetons_prompt"],
                     "jetons_reponse": rep["jetons_reponse"], "jetons_cache": rep["jetons_cache"],
                     "latence_ms": rep["latence_ms"], "rappel": rappel,
                     "contexte_octets": len(contexte or ""), "routage_ms": round(cout)}
            if note:
                ligne["routage"] = note
            resultats.append(ligne)
            print(f"   {cond:9} {rep['latence_ms']:6} ms  {rep['jetons_prompt']:5} jetons  "
                  f"rappel={'—' if rappel is None else ('oui' if rappel else 'non')}  "
                  f"{rep['texte'][:60]!r}")
    return resultats


# ── Phase de jugement ───────────────────────────────────────────────────────

RUBRIQUE = (
    "Tu notes une réponse à une question sur la documentation du projet Locaryn. "
    "Note 2 si la réponse contient l'information de référence, même formulée "
    "autrement ; note 1 si elle est à moitié juste, vague ou partielle ; "
    "note 0 si elle est fausse, hors sujet, ou si elle prétend savoir alors "
    "que la réponse de référence dit que l'information est absente. "
    "Réponds UNIQUEMENT par un objet JSON : {\"note\": 0, 1 ou 2}."
)


def phase_judge(base: str, questions: list[dict], resultats: list[dict]) -> list[dict]:
    ref = {q["id"]: q for q in questions}
    paires = list(resultats)
    random.Random(SEED).shuffle(paires)  # à l'aveugle : ni l'ordre des conditions ni les noms
    juges = []
    for i, r in enumerate(paires, 1):
        q = ref[r["question"]]
        utilisateur = (f"Question : {q['question']}\n\nRéponse de référence : {q['reponse_ref']}\n\n"
                       f"Réponse à noter : {r['reponse'] or '(vide)'}")
        try:
            rep = _post(f"{base}/v1/chat/completions", {
                "model": "juge",
                "messages": [{"role": "system", "content": RUBRIQUE},
                             {"role": "user", "content": utilisateur}],
                "temperature": 0.0, "max_tokens": 40,
                "chat_template_kwargs": {"enable_thinking": False},
                "response_format": {"type": "json_object"},
            })
            texte = rep["choices"][0]["message"]["content"]
            note = int(re.search(r"\d", texte).group())
        except Exception as e:  # noqa: BLE001 — une note perdue ne doit pas tuer la salve
            print(f"  ! jugement {r['question']}/{r['condition']} : {e}")
            note = None
        juges.append({"question": r["question"], "condition": r["condition"], "note": note})
        if i % 20 == 0:
            print(f"  {i}/{len(paires)} notées")
    return juges


# ── Rapport ─────────────────────────────────────────────────────────────────

def phase_report(index: dict, resultats: list[dict], juges: list[dict]) -> str:
    notes = {(j["question"], j["condition"]): j["note"] for j in juges}
    par_cond: dict[str, list[dict]] = defaultdict(list)
    for r in resultats:
        par_cond[r["condition"]].append(r)

    lignes = ["| Voie | Note /2 | Correctes | Rappel | Jetons prompt | Latence ms |",
              "|---|---|---|---|---|---|"]
    for cond in ["sans", "cag", "rag", "kag", "graphrag", "hybride", "laya"]:
        rs = par_cond.get(cond)
        if not rs:
            continue
        ns = [notes.get((r["question"], cond)) for r in rs]
        ns = [n for n in ns if n is not None]
        rappels = [r["rappel"] for r in rs if r["rappel"] is not None]
        lignes.append(
            f"| {cond} | {statistics.mean(ns):.2f} | "
            f"{sum(1 for n in ns if n == 2)}/{len(ns)} | "
            f"{(100 * sum(rappels) / len(rappels)):.0f}% | "
            f"{statistics.mean(r['jetons_prompt'] for r in rs):.0f} | "
            f"{statistics.mean(r['latence_ms'] for r in rs):.0f} |")

    cats: dict[str, dict[str, list[int]]] = defaultdict(lambda: defaultdict(list))
    for r in resultats:
        n = notes.get((r["question"], r["condition"]))
        if n is not None:
            cats[r["categorie"]][r["condition"]].append(n)
    secondes = ["| Catégorie | " + " | ".join(cats["fait"]) + " |", "|---|" + "---|" * len(cats["fait"])]
    for cat in ["fait", "relation", "synthese", "hors-corpus"]:
        if cat not in cats:
            continue
        secondes.append(f"| {cat} | "
                        + " | ".join(f"{statistics.mean(cats[cat][c]):.1f}" for c in cats["fait"]) + " |")

    cout = index["cout_index"]
    texte = "\n".join([
        "# Banc RAG / cAG / KAG / GraphRAG — documentation Locaryn",
        "",
        f"Corpus : {len(index['morceaux'])} morceaux, "
        f"{sum(len(m['texte']) for m in index['morceaux']) // 3} jetons estimés. "
        f"Cœur cAG : ≈{len(index['coeur_texte']) // 3} jetons. "
        f"K remontés par le RAG : {K_RAG}.",
        "",
        "## Vue d'ensemble",
        "",
        *lignes,
        "",
        "## Par catégorie de question",
        "",
        *secondes,
        "",
        "## Coût d'indexation payé en amont",
        "",
        f"- Plongements des {len(index['morceaux'])} morceaux : "
        f"{index['cout_plongement_s']} s (une seule fois).",
        f"- GraphRAG : {cout['appels']} résumés de communautés, {cout['jetons_prompt']} "
        f"jetons d'entrée + {cout['jetons_reponse']} de sortie, {cout['secondes']:.0f} s.",
        "- cAG : aucun coût d'indexation, mais le cœur occupe la fenêtre à chaque requête.",
        "- KAG : aucun appel de modèle à l'indexation, extraction par règles.",
    ])
    DOSSIER.mkdir(exist_ok=True)
    (DOSSIER / "rapport.md").write_text(texte, encoding="utf-8")
    (DOSSIER / "resultats.json").write_text(json.dumps(resultats, ensure_ascii=False, indent=1),
                                            encoding="utf-8")
    return texte


# ── Point d'entrée ──────────────────────────────────────────────────────────

def main() -> int:
    # La console Windows est en cp1252 ; les rapports et les réponses du modèle
    # sont en UTF-8. On aligne la sortie plutôt que de perdre le premier « ≈ ».
    for flux in (sys.stdout, sys.stderr):
        try:
            flux.reconfigure(encoding="utf-8")
        except Exception:  # noqa: BLE001 — un flux sans reconfigure n'est pas bloquant
            pass
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("phase", choices=["index", "run", "judge", "report", "all"])
    ap.add_argument("--base", default="http://127.0.0.1:8099")
    ap.add_argument("--only", default=None, help="une seule question (F1, R2…)")
    ap.add_argument("--embed", default=None,
                    help="serveur des plongements s'il diffère du serveur de chat "
                         "(un modèle de plongement dédié, pas le modèle de chat)")
    ap.add_argument("--conds", default=None,
                    help="conditions à mesurer, séparées par des virgules ; les résultats "
                         "s'ajoutent à ceux déjà présents")
    args = ap.parse_args()

    DOSSIER.mkdir(exist_ok=True)
    questions = json.loads((RACINE / "questions.json").read_text(encoding="utf-8"))["questions"]
    chemin_index, chemin_res = DOSSIER / "index.json", DOSSIER / "resultats.json"

    if args.phase in ("index", "all"):
        index = indexation(args.base, args.embed)
        chemin_index.write_text(json.dumps(index, ensure_ascii=False), encoding="utf-8")
        if args.phase == "index":
            return 0
    else:
        index = json.loads(chemin_index.read_text(encoding="utf-8"))

    if args.phase in ("run", "all"):
        conds = [c.strip() for c in args.conds.split(",") if c.strip()] if args.conds else None
        resultats = phase_run(args.base, index, questions, args.only, conds, args.embed)
        if conds and chemin_res.exists():
            # On ajoute aux mesures déjà là plutôt que de les écraser : rejouer
            # une condition ne doit pas coûter les six autres.
            anciennes = json.loads(chemin_res.read_text(encoding="utf-8"))
            gardes = [r for r in anciennes
                      if r["condition"] not in conds
                      or (args.only and r["question"] != args.only)]
            resultats = gardes + resultats
        chemin_res.write_text(json.dumps(resultats, ensure_ascii=False, indent=1), encoding="utf-8")
        if args.phase == "run":
            return 0
    else:
        resultats = json.loads(chemin_res.read_text(encoding="utf-8")) if chemin_res.exists() else []

    if args.phase in ("judge", "all"):
        juges = phase_judge(args.base, questions, resultats)
        (DOSSIER / "juges.json").write_text(json.dumps(juges, ensure_ascii=False, indent=1),
                                            encoding="utf-8")
        if args.phase == "judge":
            return 0
    else:
        chemin_j = DOSSIER / "juges.json"
        juges = json.loads(chemin_j.read_text(encoding="utf-8")) if chemin_j.exists() else []

    print(phase_report(index, resultats, juges))
    return 0


if __name__ == "__main__":
    sys.exit(main())
