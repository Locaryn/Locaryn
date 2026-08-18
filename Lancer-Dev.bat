@echo off
title Locaryn - Mode Developpeur
cd /d "%~dp0"
echo ===================================================
echo   Lancement de Locaryn en mode Developpeur...
echo ===================================================
pnpm tauri:dev
pause
