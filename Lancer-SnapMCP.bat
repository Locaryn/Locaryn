@echo off
setlocal
chcp 65001 >nul
title SnapMCP - Lancement du banc de test

cd /d "%~dp0.snap-astreinte"
if not exist "Lancer-Banc-SnapMCP.bat" (
    echo ERREUR : dossier .snap-astreinte introuvable.
    pause
    exit /b 1
)

call "Lancer-Banc-SnapMCP.bat"
endlocal
