#!/usr/bin/env python3
"""Chaîne CDM — les petits modèles auto-hébergés de Locaryn.

Reproduit localement la chaîne de modèles que les grands agents utilisent en
interne. Aucune API externe : tout tourne sur la machine, dans l'environnement
Python géré par l'application (`python-env`), avec les poids de `hf_cache`.

  route       Laya — classification, routage, triage d'un message.
  extraire    GLiNER2.5 — champs d'un texte ou d'un PDF (fastino/gliner2.5-
              multi-v1, multilingue, sans entraînement ni schéma figé).
  structure   Qwen (1,7–4 B, GGUF servi par llama-server) — JSON contraint
              par grammaire (`response_format: json_schema`) : c'est llama.cpp
              qui joue le rôle d'Outlines/XGrammar, décodage contraint au
              token près, sans modèle supplémentaire.
  compresser  LLMLingua-2 — compression du contexte des agents (token-level,
              ~3× plus rapide que le résumé par le modèle de conversation).
  serve       Mini-serveur HTTP local (127.0.0.1) exposant les quatre.

Chaque commande imprime du JSON sur la sortie standard.

Exemples :
  python cdm.py route "ouvre le site dans mon navigateur" --labels navigateur,ecriture
  python cdm.py extraire --fichier note.pdf --champs "personne:L'agent" "date:Date"
  python cdm.py structure --schema schema.json "liste les tâches en JSON"
  python cdm.py compresser --fichier contexte.txt --taux 0.4
  python cdm.py serve --port 8208
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.request

# ── Modèles ────────────────────────────────────────────────────────────────
MODELE_GLINER = "fastino/gliner2.5-multi-v1"
# Le seul poids LLMLingua-2 public de Microsoft : multilingue (mieux pour le
# français que l'anglais dureader, rendu en 401), 611k téléchargements.
MODELE_LINGUA = "microsoft/llmlingua-2-bert-base-multilingual-cased-meetingbank"
MODELE_LAYA = ("convaiinnovations/laya", "multilingual")

# Deux phrases sentinelles : Laya rend une probabilité sans dire de quel côté
# est le « vrai » ; l'une franchement vraie, l'autre franchement fausse, fixent
# le sens une fois pour toutes (mêmes phrases que le banc Locaryn).
SENTINELLE_VRAI = "Ce texte parle explicitement de configuration et de serveur."
SENTINELLE_FAUX = "Ce texte parle uniquement de recettes de cuisine et de jardinage."


def _json(obj) -> None:
    print(json.dumps(obj, ensure_ascii=False))


def _erreur(msg: str, code: int = 1) -> None:
    _json({"erreur": msg})
    sys.exit(code)


def _hf_home_si_absent() -> None:
    """Le poids pèsent plusieurs centaines de Mo : l'application donne
    `HF_HOME` à ses enfants Python ; lancé à la main, on retombe sur le
    `hf_cache` à côté de l'environnement (même dossier, même volume)."""
    if os.environ.get("HF_HOME"):
        return
    candidat = os.path.join(os.path.dirname(sys.prefix), "hf_cache")
    if os.path.isdir(candidat):
        os.environ["HF_HOME"] = candidat


def _lire_texte(fichier: str | None, texte: str | None) -> str:
    if texte is not None:
        return texte
    if not fichier:
        _erreur("il faut un fichier (--fichier) ou un texte (--texte)")
    if fichier == "-":
        return sys.stdin.read()
    bas = fichier.lower()
    if bas.endswith(".pdf"):
        from pypdf import PdfReader  # import tardif : pypdf est petit mais inutile ailleurs

        lecteur = PdfReader(fichier)
        return "\n".join((page.extract_text() or "") for page in lecteur.pages)
    with open(fichier, encoding="utf-8", errors="replace") as f:
        return f.read()


# ── Laya : classification, routage, triage ─────────────────────────────────
_agent_laya = None


def agent_laya():
    global _agent_laya
    if _agent_laya is None:
        import laya  # import tardif : Laya pèse ~650 Mo en mémoire

        nom, sous = MODELE_LAYA
        _agent_laya = laya.load(nom, subfolder=sous, device="cpu")
    return _agent_laya


def sens_noul(agent, labels: dict[str, dict]) -> bool:
    """Vrai si `noul` montre la vérité (phrases sentinelles)."""
    pv = _noul_moyen(agent.predict(SENTINELLE_VRAI, labels))
    pf = _noul_moyen(agent.predict(SENTINELLE_FAUX, labels))
    return pv > pf


def _noul_moyen(reponse: dict) -> float:
    rep = reponse.get("answers") or {}
    vals = [v["noul"] for v in rep.values() if isinstance(v, dict) and "noul" in v]
    return sum(vals) / len(vals) if vals else 0.5


def route(texte: str, labels: dict[str, dict]) -> dict:
    """Trie `texte` parmi les étiquettes (questions au format Laya). Renvoie les
    scores calibrés et l'étiquette retenue — c'est le triage : quel agent/voie/
    outil doit prendre la main."""
    agent = agent_laya()
    brut = agent.predict(texte, labels).get("answers") or {}
    sens = sens_noul(agent, labels)
    scores = {
        k: (v.get("noul", 0.5) if sens else 1.0 - v.get("noul", 0.5))
        for k, v in brut.items()
        if isinstance(v, dict)
    }
    retenu = max(scores, key=scores.get) if scores else None
    return {"retenu": retenu, "scores": scores, "sens_noul": sens}


# ── GLiNER2.5 : extraction de champs ───────────────────────────────────────
_model_gliner = None


def model_gliner():
    global _model_gliner
    if _model_gliner is None:
        from gliner2 import AutoExtractor  # import tardif : ~1 Go de poids

        _model_gliner = AutoExtractor.from_pretrained(MODELE_GLINER, map_location="cpu")
    return _model_gliner


def extraire(texte: str, champs: dict[str, str | None]) -> dict:
    """Extrait les champs (étiquettes libres, éventuellement décrits) d'un
    texte — GLiNER2.5 fait tenir n'importe quelle longueur de span, sans
    extracteur entraîné au préalable sur le domaine."""
    modele = model_gliner()
    labels = {k: v for k, v in champs.items() if v} or list(champs)
    res = modele.extract_entities(texte, labels, include_confidence=True, include_spans=True)
    return dict(res)


# ── Qwen + JSON contraint (llama-server) ───────────────────────────────────
def structure(
    base: str,
    consigne: str,
    schema: dict,
    max_tokens: int = 1024,
    modele: str = "local",
) -> dict:
    """Demande à Qwen un JSON conforme à `schema` : la grammaire du serveur
    (json_schema) interdit tout token hors schéma — sortie toujours valide,
    pas de « je réessaie en JSON » après coup."""
    corps = {
        "model": modele,
        "messages": [
            {
                "role": "system",
                "content": "Réponds uniquement par un objet JSON conforme au schéma demandé.",
            },
            {"role": "user", "content": consigne},
        ],
        "temperature": 0.0,
        "max_tokens": max_tokens,
        "stream": False,
        "reasoning_budget": 0,
        "chat_template_kwargs": {"enable_thinking": False},
        "response_format": {
            "type": "json_schema",
            "json_schema": {"name": "sortie", "schema": schema, "strict": True},
        },
    }
    url = base.rstrip("/") + "/v1/chat/completions"
    req = urllib.request.Request(
        url, data=json.dumps(corps).encode(), headers={"Content-Type": "application/json"}
    )
    try:
        with urllib.request.urlopen(req, timeout=600) as r:
            rep = json.load(r)
    except Exception as e:  # noqa: BLE001 — l'erreur du moteur est le résultat
        _erreur(f"moteur injoignable ou refus ({base}) : {e}")
    contenu = rep["choices"][0]["message"].get("content") or ""
    try:
        return json.loads(contenu)
    except json.JSONDecodeError:
        # La grammaire garantit du JSON ; un modèle qui outrepasserait le
        # format (ancien serveur) est renvoyé tel quel pour diagnostic.
        return {"_texte_brut": contenu}


# ── LLMLingua : compression du contexte ────────────────────────────────────
_compressor = None


def compressor():
    global _compressor
    if _compressor is None:
        from llmlingua import PromptCompressor  # import tardif : transformers

        # use_llmlingua2=True charge le poids comme classifieur de jetons
        # (BERT) ; `model_name` est le seul endroit où le nommer —
        # `llmlingua2_config` ne prend pas de modèle.
        _compressor = PromptCompressor(
            model_name=MODELE_LINGUA,
            device_map="cpu",
            use_llmlingua2=True,
        )
    return _compressor


def compresser(texte: str, taux: float, question: str = "") -> dict:
    """Comprime `texte` en gardant les jetons qui comptent (LLMLingua-2,
    entraîné à distiller GPT-4) : pour un agent, garder 40 % du contexte de
    travail plutôt que de le résumer — plus fidèle et sans modèle de chat."""
    comp = compressor()
    if getattr(comp, "use_llmlingua2", False):
        # LLMLingua-2 n'a pas de « question guide » : il juge chaque jeton
        # seul (d'où sa vitesse, ~3× le résumé par le modèle de conversation)
        # — `question` est acceptée mais sans effet sur ce moteur.
        res = comp.compress_prompt_llmlingua2([texte], rate=taux)
    else:
        res = comp.compress_prompt(texte, instruction=question, rate=taux)
    if isinstance(res, dict):
        out = res.get("compressed_prompt") or res.get("compressed_output") or ""
        info = {"texte": out, "taux_reel": res.get("rate", taux)}
        # Les compteurs réels du moteur (mesurés sur ses propres jetons) valent
        # mieux que le taux demandé.
        if "origin_tokens" in res and "compressed_tokens" in res:
            info["jetons_avant"] = res["origin_tokens"]
            info["jetons_apres"] = res["compressed_tokens"]
        return info
    return {"texte": str(res), "taux_reel": taux}


# ── Commandes (CLI) ────────────────────────────────────────────────────────
def _labels_de(paires: list[str]) -> dict[str, dict]:
    """`nom` ou `nom=Question posée à Laya` → le dict attendu par `predict`.
    Sans question explicite, c'est le nom qui sert d'instruction."""
    out: dict[str, dict] = {}
    for p in paires:
        nom, sep, question = p.partition("=")
        nom = nom.strip()
        if not nom:
            continue
        out[nom] = {"type": "noul", "instructions": (question.strip() if sep else nom)}
    return out


def _champs_de(paires: list[str]) -> dict[str, str | None]:
    """`nom`, `nom=Description` ou `nom:Description` → {nom: description}."""
    champs: dict[str, str | None] = {}
    for p in paires:
        nom, sep, desc = p.partition("=")
        if not sep:
            nom, _, desc = p.partition(":")
        champs[nom.strip()] = desc.strip() or None
    return champs


def cmd_route(args) -> None:
    texte = _lire_texte(args.fichier, args.texte)
    _json(route(texte, _labels_de(args.labels)))


def cmd_extraire(args) -> None:
    texte = _lire_texte(args.fichier, args.texte)
    _json(extraire(texte, _champs_de(args.champs)))


def cmd_structure(args) -> None:
    try:
        if args.schema_fichier:
            with open(args.schema_fichier, encoding="utf-8") as f:
                schema = json.load(f)
        else:
            schema = json.loads(args.schema_json)
    except (OSError, json.JSONDecodeError) as e:
        _erreur(f"schéma illisible : {e}")
    _json(structure(args.base, args.consigne, schema, args.max_tokens))


def cmd_compresser(args) -> None:
    texte = _lire_texte(args.fichier, args.texte)
    _json(compresser(texte, args.taux))


def cmd_ping(args) -> None:
    """Charger chaque brique et le dire : l'installation est vérifiable en
    une commande (`cdm.py ping` → JSON avec l'état de chaque modèle)."""
    etat: dict[str, str] = {}
    try:
        agent_laya()
        etat["laya"] = "pret"
    except Exception as e:  # noqa: BLE001
        etat["laya"] = f"echec : {e}"
    try:
        model_gliner()
        etat["gliner"] = "pret"
    except Exception as e:  # noqa: BLE001
        etat["gliner"] = f"echec : {e}"
    try:
        compressor()
        etat["llmlingua"] = "pret"
    except Exception as e:  # noqa: BLE001
        etat["llmlingua"] = f"echec : {e}"
    try:
        import urllib.error

        urllib.request.urlopen(args.base.rstrip("/") + "/props", timeout=3)
        etat["qwen"] = f"pret ({args.base})"
    except Exception as e:  # noqa: BLE001
        etat["qwen"] = f"hors ligne ({args.base}) : {e}"
    _json(etat)


def cmd_serve(args) -> None:
    """Serveur HTTP local : les agents parlent JSON, sans lancer Python à
    chaque appel (le chargement des poids paierait à chaque fois)."""
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

    class H(BaseHTTPRequestHandler):
        def log_message(self, *a) -> None:  # pas de bruit sur la sortie
            pass

        def _rep(self, obj, code: int = 200) -> None:
            corps = json.dumps(obj, ensure_ascii=False).encode()
            self.send_response(code)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(corps)))
            self.end_headers()
            self.wfile.write(corps)

        def do_GET(self) -> None:
            if self.path in ("/", "/health"):
                self._rep({"ok": True, "service": "cdm"})
            else:
                self._rep({"erreur": "inconnu"}, 404)

        def do_POST(self) -> None:
            try:
                n = int(self.headers.get("Content-Length") or 0)
                corps = json.loads(self.rfile.read(n) or b"{}")
                if self.path == "/route":
                    self._json(route(corps["texte"], corps["labels"]))
                elif self.path == "/extraire":
                    self._json(extraire(corps["texte"], corps["champs"]))
                elif self.path == "/structure":
                    self._json(
                        structure(
                            corps.get("base", args.base),
                            corps["consigne"],
                            corps["schema"],
                            corps.get("max_tokens", 1024),
                        )
                    )
                elif self.path == "/compresser":
                    self._json(
                        compresser(
                            corps["texte"],
                            float(corps.get("taux", 0.5)),
                            corps.get("question", ""),
                        )
                    )
                else:
                    self._rep({"erreur": f"route inconnue : {self.path}"}, 404)
            except KeyError as e:
                self._rep({"erreur": f"champ manquant : {e}"}, 400)
            except Exception as e:  # noqa: BLE001 — l'erreur part au client
                self._rep({"erreur": str(e)}, 500)

        def _json(self, obj) -> None:
            self._rep(obj)

    host = "127.0.0.1"
    srv = ThreadingHTTPServer((host, args.port), H)
    _json({"ok": True, "url": f"http://{host}:{args.port}"})
    srv.serve_forever()


def main() -> int:
    for flux in (sys.stdout, sys.stderr):
        try:
            flux.reconfigure(encoding="utf-8")
        except Exception:  # noqa: BLE001 — un flux sans reconfigure n'est pas bloquant
            pass
    _hf_home_si_absent()

    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sous = ap.add_subparsers(dest="cmd", required=True)

    p = sous.add_parser("route", help="classification / routage / triage (Laya)")
    g = p.add_mutually_exclusive_group(required=True)
    g.add_argument("--texte", help="texte à trier")
    g.add_argument("--fichier", help="fichier à lire (.txt, .pdf, ou - pour l'entrée standard)")
    p.add_argument(
        "--labels",
        required=True,
        nargs="+",
        help='étiquettes : "nom" ou "nom=Question pour Laya"',
    )
    p.set_defaults(func=cmd_route)

    p = sous.add_parser("extraire", help="extraction de champs (GLiNER2.5)")
    g = p.add_mutually_exclusive_group(required=True)
    g.add_argument("--texte")
    g.add_argument("--fichier", help=".txt, .pdf, ou - pour l'entrée standard")
    p.add_argument(
        "--champs",
        required=True,
        nargs="+",
        help='champs "nom" ou "nom:Description"',
    )
    p.set_defaults(func=cmd_extraire)

    p = sous.add_parser("structure", help="JSON contraint (Qwen via llama-server)")
    p.add_argument("consigne")
    g = p.add_mutually_exclusive_group(required=True)
    g.add_argument("--schema-fichier", help="fichier JSON du schéma")
    g.add_argument("--schema-json", help="schéma JSON en ligne")
    p.add_argument("--base", default=os.environ.get("CDM_BASE", "http://127.0.0.1:8080"))
    p.add_argument("--max-tokens", type=int, default=1024)
    p.set_defaults(func=cmd_structure)

    p = sous.add_parser("compresser", help="compression de contexte (LLMLingua-2)")
    g = p.add_mutually_exclusive_group(required=True)
    g.add_argument("--texte")
    g.add_argument("--fichier", help=".txt, .pdf, ou - pour l'entrée standard")
    p.add_argument("--taux", type=float, default=0.5, help="part de jetons à garder (0.5 = 50 %)")
    p.set_defaults(func=cmd_compresser)

    p = sous.add_parser("ping", help="vérifier que chaque brique charge")
    p.add_argument("--base", default=os.environ.get("CDM_BASE", "http://127.0.0.1:8080"))
    p.set_defaults(func=cmd_ping)

    p = sous.add_parser("serve", help="serveur HTTP local des quatre briques")
    p.add_argument("--port", type=int, default=8208)
    p.add_argument("--base", default=os.environ.get("CDM_BASE", "http://127.0.0.1:8080"),
                   help="endpoint du modèle de chat pour /structure")
    p.set_defaults(func=cmd_serve)

    args = ap.parse_args()
    args.func(args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
