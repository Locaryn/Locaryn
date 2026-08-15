@echo off
setlocal enabledelayedexpansion

echo ========================================================
echo Pushing Locaryn Core and all 13 Official Plugins
echo ========================================================

cd d:\Documents\Syncho
git remote set-url origin https://github.com/Locaryn/locaryn.git
git add .
git commit -m "refactor: modularize official extensions, dynamic UI filtering and CI releases"
git push -u origin main

set PLUGINS=plugin-image-gen plugin-image-editor plugin-video-gen plugin-3d-gen plugin-voice-tts plugin-music-gen plugin-vision-ocr plugin-rag-qa plugin-translation plugin-text-analysis plugin-ssh plugin-travel-tunnel plugin-model-training

for %%P in (%PLUGINS%) do (
    echo.
    echo ========================================================
    echo Pushing %%P to https://github.com/Locaryn/%%P.git
    echo ========================================================
    cd d:\Documents\Syncho\plugins\%%P
    if exist .git (
        git add .
        git commit -m "release: initial official plugin release"
        git branch -M main
        git remote set-url origin https://github.com/Locaryn/%%P.git
        git push -u origin main --force
    ) else (
        git init -b main
        git add .
        git commit -m "release: initial official plugin release"
        git remote add origin https://github.com/Locaryn/%%P.git
        git push -u origin main --force
    )
)

cd d:\Documents\Syncho
echo.
echo ========================================================
echo Done! All repositories are published on GitHub.
echo ========================================================
