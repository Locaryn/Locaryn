#!/usr/bin/env bash
# Dépose le service et la CLI là où Tauri va les chercher pour les embarquer
# avec l'application (`bundle.externalBin` dans tauri.conf.json).
#
# Pourquoi ils voyagent avec l'application : la case « serveur partagé » lance
# `locaryn-daemon` posé à côté de l'exécutable (apps/desktop/src-tauri/src/
# server_mode.rs). Sans ces fichiers, la fonction ne peut qu'annoncer un service
# introuvable — et l'installation ne donne pas non plus la commande `locaryn`.
#
# Le même script sert à la CI et à la release, pour que les deux ne puissent pas
# diverger : la CI compile en debug (il s'agit seulement de vérifier que tout
# tient debout), la release en release.
#
#   bash scripts/stage-desktop-sidecars.sh              # release, cible hôte
#   PROFILE=debug bash scripts/stage-desktop-sidecars.sh
#   UNIVERSAL=1 bash scripts/stage-desktop-sidecars.sh  # macOS, bundle universel

set -euo pipefail

cd "$(dirname "$0")/.."

profile="${PROFILE:-release}"
out="apps/desktop/src-tauri/binaries"
mkdir -p "$out"

flags=()
subdir="debug"
if [ "$profile" = "release" ]; then
  flags+=(--release)
  subdir="release"
fi

ext=""
case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*) ext=".exe" ;;
esac

if [ "${UNIVERSAL:-0}" = "1" ]; then
  # Le bundle macOS est universel : ses compagnons doivent l'être aussi, sinon
  # la moitié des Mac trouve un binaire qu'ils ne savent pas exécuter.
  cargo build "${flags[@]}" --target x86_64-apple-darwin -p locaryn-daemon -p locaryn-cli
  cargo build "${flags[@]}" --target aarch64-apple-darwin -p locaryn-daemon -p locaryn-cli
  for bin in locaryn-daemon locaryn; do
    lipo -create \
      "target/x86_64-apple-darwin/$subdir/$bin" \
      "target/aarch64-apple-darwin/$subdir/$bin" \
      -output "$out/$bin-universal-apple-darwin"
    chmod +x "$out/$bin-universal-apple-darwin"
  done
else
  cargo build "${flags[@]}" -p locaryn-daemon -p locaryn-cli
  triple=$(rustc -vV | sed -n 's/^host: //p')
  for bin in locaryn-daemon locaryn; do
    cp "target/$subdir/$bin$ext" "$out/$bin-$triple$ext"
  done
fi

ls -l "$out"
