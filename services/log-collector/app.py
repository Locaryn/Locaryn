"""Locaryn : réception des journaux envoyés par l'application.

POST /locaryn/api/logs        rapport JSON (anonyme, sans authentification)
GET  /locaryn/api/logs/health état du service
GET  /locaryn/api/logs/admin  consultation, protégée (Basic, mot de passe ADMIN_PASSWORD)

Bibliothèque standard seulement. Les adresses IP ne servent qu'à limiter le
débit, en mémoire : elles ne sont jamais écrites sur disque.
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import html
import json
import logging
import os
import re
import sqlite3
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.parse import parse_qs, urlparse

PREFIXE = "/locaryn/api/logs"
BASE = os.environ.get("DB_PATH", "/data/logs.db")
MOT_DE_PASSE = os.environ.get("ADMIN_PASSWORD", "")
TAILLE_MAX = int(os.environ.get("MAX_BYTES", str(3 * 1024 * 1024)))
RETENTION_JOURS = int(os.environ.get("RETENTION_DAYS", "30"))
BASE_MAX_OCTETS = int(os.environ.get("DB_MAX_BYTES", str(200 * 1024 * 1024)))
PAR_HEURE = int(os.environ.get("RATE_PER_HOUR", "20"))
INTERVALLE_MIN = float(os.environ.get("RATE_MIN_INTERVAL", "5"))

GENRES = {"erreur", "note", "probleme", "manuel"}
GRAVITES = {"critique", "erreur", "info"}
NOTES = {"up", "down"}

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
JOURNAL = logging.getLogger("locaryn-logs")
VERROU = threading.Lock()
VUS: dict[str, list[float]] = {}
ECHECS_ADMIN: dict[str, list[float]] = {}

# Deuxième passe de nettoyage, au cas où un client ancien aurait laissé passer quelque chose.
MASQUES = [
    (re.compile(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}"), "[e-mail]"),
    (re.compile(r"(?i)[A-Z]:\\Users\\[^\\\s\"']+"), r"~"),
    (re.compile(r"/(?:home|Users)/[^/\s\"']+"), "~"),
    (re.compile(r"(?i)(bearer\s+)[A-Za-z0-9._\-]{12,}"), r"\1[masqué]"),
    (re.compile(r"\b(sk-|hf_|gh[pousr]_|AIza|xox[abp]-)[A-Za-z0-9_\-]{10,}"), r"\1[masqué]"),
    (re.compile(r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}"), "[jeton]"),
    (re.compile(r"\b(?:10|127|192\.168|172\.(?:1[6-9]|2\d|3[01]))(?:\.\d{1,3}){2,3}\b"), "[ip]"),
]


def nettoyer(texte: str) -> str:
    for motif, remplacement in MASQUES:
        texte = motif.sub(remplacement, texte)
    return texte


def ouvrir() -> sqlite3.Connection:
    cx = sqlite3.connect(BASE, timeout=10)
    cx.row_factory = sqlite3.Row
    return cx


def preparer_base() -> None:
    os.makedirs(os.path.dirname(BASE), exist_ok=True)
    with ouvrir() as cx:
        cx.execute("PRAGMA journal_mode=WAL")
        cx.execute(
            """CREATE TABLE IF NOT EXISTS rapports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                recu INTEGER NOT NULL,
                genre TEXT NOT NULL,
                gravite TEXT NOT NULL,
                note TEXT,
                version TEXT, os TEXT, arch TEXT, modele TEXT,
                morphs TEXT, titre TEXT, detail TEXT, journaux TEXT, echange TEXT,
                taille INTEGER NOT NULL
            )"""
        )
        cx.execute("CREATE INDEX IF NOT EXISTS i_recu ON rapports(recu)")
        cx.execute("CREATE INDEX IF NOT EXISTS i_filtres ON rapports(gravite, note)")


def purger() -> None:
    limite = int(time.time()) - RETENTION_JOURS * 86_400
    with ouvrir() as cx:
        cx.execute("DELETE FROM rapports WHERE recu < ?", (limite,))
        total = cx.execute("SELECT COALESCE(SUM(taille), 0) FROM rapports").fetchone()[0]
        while total > BASE_MAX_OCTETS:
            ligne = cx.execute("SELECT id, taille FROM rapports ORDER BY id LIMIT 1").fetchone()
            if ligne is None:
                break
            cx.execute("DELETE FROM rapports WHERE id = ?", (ligne["id"],))
            total -= ligne["taille"]


def boucle_purge() -> None:
    while True:
        try:
            purger()
        except sqlite3.Error as e:
            JOURNAL.error("purge impossible : %s", e)
        time.sleep(3600)


def autorise(table: dict[str, list[float]], ip: str, par_heure: int, intervalle: float) -> bool:
    maintenant = time.time()
    with VERROU:
        recents = [t for t in table.get(ip, []) if maintenant - t < 3600]
        if len(recents) >= par_heure or (recents and maintenant - recents[-1] < intervalle):
            table[ip] = recents
            return False
        recents.append(maintenant)
        table[ip] = recents
        if len(table) > 50_000:
            table.clear()
        return True


def texte(v: Any, limite: int) -> str | None:
    if v is None:
        return None
    if not isinstance(v, str):
        v = json.dumps(v, ensure_ascii=False)
    return nettoyer(v[:limite])


def choisir(v: Any, permis: set[str], defaut: str | None) -> str | None:
    return v if isinstance(v, str) and v in permis else defaut


def ligne_depuis(corps: dict[str, Any], taille: int) -> tuple[Any, ...]:
    genre = choisir(corps.get("genre"), GENRES, "manuel")
    gravite = choisir(corps.get("gravite"), GRAVITES, "info")
    morphs = corps.get("morphs")
    if not isinstance(morphs, list):
        morphs = []
    echange = corps.get("echange")
    if genre not in ("note", "probleme"):
        echange = None
    return (
        int(time.time()),
        genre,
        gravite,
        choisir(corps.get("note"), NOTES, None),
        texte(corps.get("version"), 40),
        texte(corps.get("os"), 20),
        texte(corps.get("arch"), 20),
        texte(corps.get("modele"), 200),
        texte(morphs[:100], 8_000),
        texte(corps.get("titre"), 300),
        texte(corps.get("detail"), 20_000),
        texte(corps.get("journaux"), TAILLE_MAX),
        texte(echange, 200_000) if echange is not None else None,
        taille,
    )


def enregistrer(corps: dict[str, Any], taille: int) -> int:
    with ouvrir() as cx:
        cur = cx.execute(
            """INSERT INTO rapports (recu, genre, gravite, note, version, os, arch, modele,
               morphs, titre, detail, journaux, echange, taille)
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",
            ligne_depuis(corps, taille),
        )
        return int(cur.lastrowid or 0)


FILTRES = {"genre": GENRES, "gravite": GRAVITES, "note": NOTES}


def chercher(q: dict[str, list[str]], limite: int = 200) -> list[sqlite3.Row]:
    clauses: list[str] = []
    valeurs: list[Any] = []
    for champ, permis in FILTRES.items():
        v = (q.get(champ) or [""])[0]
        if v in permis:
            clauses.append(f"{champ} = ?")
            valeurs.append(v)
    version = (q.get("version") or [""])[0]
    if version:
        clauses.append("version = ?")
        valeurs.append(version)
    mot = (q.get("q") or [""])[0]
    if mot:
        clauses.append("(titre LIKE ? OR detail LIKE ? OR modele LIKE ?)")
        valeurs.extend([f"%{mot}%"] * 3)
    ou = f"WHERE {' AND '.join(clauses)}" if clauses else ""
    with ouvrir() as cx:
        return cx.execute(
            f"SELECT id, recu, genre, gravite, note, version, os, arch, modele, titre "
            f"FROM rapports {ou} ORDER BY id DESC LIMIT ?",
            (*valeurs, limite),
        ).fetchall()


def un_rapport(ident: int) -> sqlite3.Row | None:
    with ouvrir() as cx:
        return cx.execute("SELECT * FROM rapports WHERE id = ?", (ident,)).fetchone()


STYLE = """body{font:14px/1.5 system-ui,sans-serif;margin:0;padding:16px;background:#f6f6f4;color:#1c1c1a}
@media(prefers-color-scheme:dark){body{background:#151514;color:#e8e8e4}a{color:#8fbf9a}
table,pre,form{background:#1e1e1c!important}}a{color:#3d7a4e}
table{border-collapse:collapse;width:100%;background:#fff}td,th{padding:8px;border-bottom:1px solid #8883;text-align:left;vertical-align:top}
form{display:flex;flex-wrap:wrap;gap:8px;padding:12px;background:#fff;margin-bottom:12px}
select,input,button{min-height:40px;padding:0 10px;font:inherit}pre{white-space:pre-wrap;word-break:break-word;background:#fff;padding:12px;overflow-x:auto}
.c{color:#b4442c;font-weight:600}.wrap{max-width:1200px;margin:auto}"""


def page(titre: str, corps: str) -> bytes:
    return (
        f"<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>"
        f"<title>{html.escape(titre)}</title><style>{STYLE}</style><div class=wrap>{corps}</div>"
    ).encode()


def option(nom: str, valeurs: list[tuple[str, str]], courant: str) -> str:
    opts = "".join(
        f"<option value='{v}'{' selected' if v == courant else ''}>{html.escape(l)}</option>"
        for v, l in valeurs
    )
    return f"<select name={nom}>{opts}</select>"


def page_liste(q: dict[str, list[str]]) -> bytes:
    lignes = chercher(q)
    c = lambda k: (q.get(k) or [""])[0]  # noqa: E731
    formulaire = (
        "<form>"
        + option("gravite", [("", "Toute gravité"), ("critique", "Bug critique"), ("erreur", "Erreur"), ("info", "Info")], c("gravite"))
        + option("note", [("", "Toute note"), ("up", "Pouce levé"), ("down", "Pouce baissé")], c("note"))
        + option("genre", [("", "Tout genre"), ("erreur", "Erreur"), ("note", "Note"), ("probleme", "Problème décrit"), ("manuel", "Manuel")], c("genre"))
        + f"<input name=version placeholder=version value='{html.escape(c('version'))}'>"
        + f"<input name=q placeholder=recherche value='{html.escape(c('q'))}'>"
        + "<button>Filtrer</button></form>"
    )
    rangs = "".join(
        f"<tr><td><a href='{PREFIXE}/admin/{r['id']}'>#{r['id']}</a></td>"
        f"<td>{time.strftime('%Y-%m-%d %H:%M', time.gmtime(r['recu']))}</td>"
        f"<td class='{'c' if r['gravite'] == 'critique' else ''}'>{r['gravite']}</td>"
        f"<td>{r['genre']}</td><td>{'levé' if r['note'] == 'up' else 'baissé' if r['note'] == 'down' else ''}</td>"
        f"<td>{html.escape(r['version'] or '')}</td><td>{html.escape(r['modele'] or '')}</td>"
        f"<td>{html.escape(r['titre'] or '')}</td></tr>"
        for r in lignes
    )
    tableau = (
        "<table><tr><th>#</th><th>Reçu (UTC)</th><th>Gravité</th><th>Genre</th><th>Note</th>"
        f"<th>Version</th><th>Modèle</th><th>Titre</th></tr>{rangs}</table>"
    )
    return page("Journaux Locaryn", f"<h1>Journaux Locaryn ({len(lignes)})</h1>{formulaire}{tableau}")


def page_detail(r: sqlite3.Row) -> bytes:
    champs = ["genre", "gravite", "note", "version", "os", "arch", "modele", "titre"]
    entete = "".join(f"<tr><th>{k}</th><td>{html.escape(str(r[k] or ''))}</td></tr>" for k in champs)
    blocs = "".join(
        f"<h2>{k}</h2><pre>{html.escape(r[k])}</pre>"
        for k in ("morphs", "echange", "detail", "journaux")
        if r[k]
    )
    retour = f"<p><a href='{PREFIXE}/admin'>← Liste</a></p>"
    return page(f"Rapport #{r['id']}", f"{retour}<h1>Rapport #{r['id']}</h1><table>{entete}</table>{blocs}")


class Gestionnaire(BaseHTTPRequestHandler):
    server_version = "locaryn-logs"
    sys_version = ""

    def log_message(self, format: str, *args: Any) -> None:  # noqa: A002
        # Pas d'IP dans le journal du service.
        JOURNAL.info("%s %s", self.command, self.path.split("?")[0])

    def ip(self) -> str:
        transmis = self.headers.get("X-Forwarded-For", "")
        brut = transmis.split(",")[-1].strip() if transmis else self.client_address[0]
        return hashlib.sha256(brut.encode()).hexdigest()[:16]

    def repondre(self, code: int, corps: bytes, type_: str = "application/json") -> None:
        self.send_response(code)
        self.send_header("Content-Type", f"{type_}; charset=utf-8")
        self.send_header("Content-Length", str(len(corps)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        if type_ == "text/html":
            self.send_header("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'")
        self.end_headers()
        self.wfile.write(corps)

    def json(self, code: int, valeur: dict[str, Any]) -> None:
        self.repondre(code, json.dumps(valeur, ensure_ascii=False).encode())

    def do_POST(self) -> None:  # noqa: N802
        if urlparse(self.path).path.rstrip("/") != PREFIXE:
            return self.json(404, {"erreur": "introuvable"})
        try:
            taille = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            return self.json(400, {"erreur": "longueur invalide"})
        if taille <= 0:
            return self.json(400, {"erreur": "corps vide"})
        if taille > TAILLE_MAX:
            return self.json(413, {"erreur": f"trop gros (max {TAILLE_MAX} octets)"})
        if not autorise(VUS, self.ip(), PAR_HEURE, INTERVALLE_MIN):
            return self.json(429, {"erreur": "trop de rapports, réessayez plus tard"})
        try:
            corps = json.loads(self.rfile.read(taille).decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as e:
            return self.json(400, {"erreur": f"JSON invalide : {e.msg if hasattr(e, 'msg') else 'encodage'}"})
        if not isinstance(corps, dict):
            return self.json(400, {"erreur": "objet JSON attendu"})
        try:
            ident = enregistrer(corps, taille)
        except sqlite3.Error as e:
            JOURNAL.error("écriture impossible : %s", e)
            return self.json(500, {"erreur": "stockage indisponible"})
        self.json(201, {"ok": True, "id": ident})

    def admin_ok(self) -> bool:
        if not MOT_DE_PASSE:
            return False
        if not autorise(ECHECS_ADMIN, self.ip(), 30, 0):
            return False
        entete = self.headers.get("Authorization", "")
        if not entete.startswith("Basic "):
            return False
        try:
            _, _, mdp = base64.b64decode(entete[6:]).decode("utf-8").partition(":")
        except (ValueError, UnicodeDecodeError):
            return False
        ok = hmac.compare_digest(mdp.encode(), MOT_DE_PASSE.encode())
        if ok:
            with VERROU:
                ECHECS_ADMIN.pop(self.ip(), None)
        return ok

    def do_GET(self) -> None:  # noqa: N802
        url = urlparse(self.path)
        chemin = url.path.rstrip("/")
        if chemin == f"{PREFIXE}/health":
            return self.json(200, {"ok": True})
        if not chemin.startswith(f"{PREFIXE}/admin"):
            return self.json(404, {"erreur": "introuvable"})
        if not self.admin_ok():
            self.send_response(401)
            self.send_header("WWW-Authenticate", 'Basic realm="Journaux Locaryn"')
            self.send_header("Content-Length", "0")
            self.end_headers()
            return None
        reste = chemin[len(f"{PREFIXE}/admin"):].strip("/")
        q = parse_qs(url.query)
        if reste == "":
            return self.repondre(200, page_liste(q), "text/html")
        if reste == "json":
            return self.json(200, {"rapports": [dict(r) for r in chercher(q, 1000)]})
        if reste.isdigit():
            r = un_rapport(int(reste))
            if r is None:
                return self.json(404, {"erreur": "introuvable"})
            return self.repondre(200, page_detail(r), "text/html")
        return self.json(404, {"erreur": "introuvable"})


def main() -> None:
    preparer_base()
    purger()
    threading.Thread(target=boucle_purge, daemon=True).start()
    if not MOT_DE_PASSE:
        JOURNAL.warning("ADMIN_PASSWORD absent : la consultation est fermée")
    serveur = ThreadingHTTPServer(("0.0.0.0", int(os.environ.get("PORT", "4510"))), Gestionnaire)
    JOURNAL.info("à l'écoute sur %s", serveur.server_address)
    serveur.serve_forever()


if __name__ == "__main__":
    main()
