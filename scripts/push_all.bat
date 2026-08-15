@echo off
setlocal enabledelayedexpansion

REM Publie le cœur Locaryn puis les 13 extensions officielles.
REM
REM Deux garde-fous par rapport à la première version de ce script :
REM   - plus de `git add .` à la racine : il avait publié dans le dépôt public
REM     des sorties de commandes, des fichiers temporaires et un .wav de test,
REM     et il aurait fini par y publier la clé privée de l'updater. Ce script
REM     pousse ce qui est *déjà* commité et s'arrête si l'arbre est sale.
REM   - plus de `push --force` : écraser l'historique distant de 13 dépôts
REM     n'est pas une opération de routine.

cd /d "%~dp0.."

echo ========================================================
echo Verification de l'arbre de travail
echo ========================================================
git diff --quiet && git diff --cached --quiet
if errorlevel 1 (
    echo.
    echo ERREUR : des modifications ne sont pas commitees. Commitez-les
    echo          d'abord, puis relancez ce script.
    git status --short
    exit /b 1
)

echo.
echo ========================================================
echo Poussee du depot principal
echo ========================================================
git push origin main
if errorlevel 1 exit /b 1

set PLUGINS=plugin-image-gen plugin-image-editor plugin-video-gen plugin-3d-gen plugin-voice-tts plugin-music-gen plugin-vision-ocr plugin-rag-qa plugin-translation plugin-text-analysis plugin-ssh plugin-travel-tunnel plugin-model-training

for %%P in (%PLUGINS%) do (
    echo.
    echo ========================================================
    echo %%P  ^>  https://github.com/Locaryn/%%P.git
    echo ========================================================
    pushd "%~dp0..\plugins\%%P"
    if exist .git (
        git add -A
        git diff --cached --quiet || git commit -m "release: mise a jour de l'extension"
        git remote set-url origin https://github.com/Locaryn/%%P.git
        git push -u origin main
    ) else (
        git init -b main
        git add -A
        git commit -m "release: initial plugin codebase"
        git remote add origin https://github.com/Locaryn/%%P.git
        git push -u origin main
    )
    popd
)

echo.
echo ========================================================
echo Termine.
echo ========================================================
