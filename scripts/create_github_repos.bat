@echo off
setlocal enabledelayedexpansion

echo ===================================================
echo 1. Pushing main core repository (Locaryn/locaryn)...
echo ===================================================
git remote set-url origin https://github.com/Locaryn/locaryn.git
git add .
git commit -m "refactor: modularize official extensions and dynamic UI gating"
git push -u origin main

echo.
echo ===================================================
echo 2. Initializing and pushing all 13 official plugins...
echo ===================================================

for %%P in (
    plugin-image-gen
    plugin-image-editor
    plugin-video-gen
    plugin-3d-gen
    plugin-voice-tts
    plugin-music-gen
    plugin-vision-ocr
    plugin-rag-qa
    plugin-translation
    plugin-text-analysis
    plugin-ssh
    plugin-travel-tunnel
    plugin-model-training
) do (
    echo.
    echo --- Publishing %%P ---
    cd d:\Documents\Syncho\plugins\%%P
    if exist .git (
        git add .
        git commit -m "release: initial plugin codebase"
        git push -u origin main
    ) else (
        git init -b main
        git add .
        git commit -m "release: initial plugin codebase"
        git remote add origin https://github.com/Locaryn/%%P.git
        git push -u origin main
    )
)

cd d:\Documents\Syncho
echo.
echo ===================================================
echo All 14 repositories successfully pushed to Locaryn!
echo ===================================================
pause
