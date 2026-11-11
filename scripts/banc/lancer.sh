#!/usr/bin/env bash
# Lance l'application de bureau sur un profil d'essai, à côté de celle de la
# personne, prête à être pilotée par `pilote.mjs`.
#
# - profil séparé (`LOCARYN_HOME`) : ni ses conversations ni ses réglages ;
# - données WebView2 séparées, débogage à distance sur le port du banc ;
# - fenêtre posée hors de l'écran : elle se dessine, mais ne gêne personne.
#
#   bash scripts/banc/lancer.sh            # profil gardé d'un essai à l'autre
#   NEUF=1 bash scripts/banc/lancer.sh     # profil vierge, « comme un PC neuf »
#   MODELES="D:/…/models" bash scripts/banc/lancer.sh   # voir de vrais modèles
#   bash scripts/banc/lancer.sh arreter
#
# Windows seulement (WebView2). La compilation se fait avant :
#   pnpm --filter @locaryn/desktop build
#   PROFILE=debug bash scripts/stage-desktop-sidecars.sh
#   (cd apps/desktop/src-tauri && cargo build --features tauri/custom-protocol)

set -euo pipefail
cd "$(dirname "$0")/../.."

BANC="${BANC:-/d/Documents/_essais/banc}"
PORT="${PORT_BANC:-9333}"
CIBLE="${CARGO_TARGET_DIR:-target}/debug"
APP="$BANC/app"

arreter() {
  [ -d "$APP" ] || return 0
  powershell -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -w scripts/banc/fenetre.ps1)"     -DossierApp "$(cygpath -w "$APP")" -Arreter
  rm -f "$BANC/pid"
  # Le temps que Windows relâche l'exécutable.
  sleep 1
}

if [ "${1:-}" = "arreter" ]; then
  arreter
  echo "banc arrêté"
  exit 0
fi

arreter
[ -f "$CIBLE/locaryn-desktop.exe" ] || { echo "application non compilée : $CIBLE/locaryn-desktop.exe" >&2; exit 1; }

if [ -n "${NEUF:-}" ]; then
  rm -rf "${BANC:?}/profil" "${BANC:?}/webview"
fi
mkdir -p "$APP" "$BANC/profil" "$BANC/webview"

# Une copie : l'exécutable compilé reste remplaçable pendant que le banc tourne.
cp "$CIBLE/locaryn-desktop.exe" "$APP/"
for compagnon in locaryn-daemon locaryn locaryn-server; do
  src="apps/desktop/src-tauri/binaries/$compagnon-x86_64-pc-windows-msvc.exe"
  [ -f "$src" ] && cp "$src" "$APP/$compagnon.exe"
done

# Une compilation de débogage ouvre une console : on passe l'exécutable en
# sous-système « fenêtre » (octet PE), comme l'application publiée.
python - "$APP/locaryn-desktop.exe" <<'PY'
import struct, sys
chemin = sys.argv[1]
with open(chemin, "r+b") as f:
    f.seek(0x3C)
    pe = struct.unpack("<I", f.read(4))[0]
    f.seek(pe + 0x5C)
    f.write(struct.pack("<H", 2))
PY

export LOCARYN_HOME
LOCARYN_HOME="$(cygpath -w "$BANC/profil")"
export WEBVIEW2_USER_DATA_FOLDER
WEBVIEW2_USER_DATA_FOLDER="$(cygpath -w "$BANC/webview")"
# Hors de l'écran, Chromium tient la fenêtre pour masquée et cesse de la
# dessiner : les captures attendaient une image qui ne venait jamais.
export WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=$PORT --disable-features=CalculateNativeWinOcclusion --disable-backgrounding-occluded-windows --disable-renderer-backgrounding"
export RUST_LOG="${RUST_LOG:-info,locaryn=debug}"
if [ -n "${MODELES:-}" ]; then
  export LOCARYN_MODELS_DIR="$MODELES"
fi

"$APP/locaryn-desktop.exe" >"$BANC/sortie.log" 2>&1 &

# Attendre que la page réponde au débogage à distance.
for _ in $(seq 1 60); do
  if curl -s -m 2 "http://127.0.0.1:$PORT/json/version" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
curl -s -m 2 "http://127.0.0.1:$PORT/json/version" >/dev/null || { echo "le banc ne répond pas (voir $BANC/sortie.log)" >&2; exit 1; }

# Son identifiant Windows (pour l'arrêter), et sa fenêtre posée hors de l'écran.
powershell -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -w scripts/banc/fenetre.ps1)"   -DossierApp "$(cygpath -w "$APP")" -FichierPid "$(cygpath -w "$BANC/pid")" >/dev/null

echo "banc prêt : http://127.0.0.1:$PORT (profil $BANC/profil)"
