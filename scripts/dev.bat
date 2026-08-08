@echo off
setlocal enabledelayedexpansion

:: Locaryn dev launcher (Windows batch).
::
:: The desktop app embeds the Locaryn core in-process, so it does NOT need the
:: daemon. By default this just starts the Tauri desktop dev server.
::   scripts\dev.bat                 :: launch the desktop app (recommended)
::   scripts\dev.bat -WithDaemon     :: also start the daemon (for CLI testing)

cd /d "%~dp0.."
set "ROOT=%cd%"
set "WITH_DAEMON="
if /I "%~1"=="-WithDaemon" set "WITH_DAEMON=1"

where cargo >nul 2>&1
if errorlevel 1 (
    echo [Locaryn] cargo not found in PATH. Install Rust: https://rustup.rs/
    exit /b 1
)
where pnpm >nul 2>&1
if errorlevel 1 (
    echo [Locaryn] pnpm not found in PATH. Install pnpm: https://pnpm.io/installation
    exit /b 1
)

if not exist "%ROOT%\tmp" mkdir "%ROOT%\tmp"
set "DAEMON_STARTED="

powershell -Command "Get-NetTCPConnection -LocalPort 1420 -ErrorAction SilentlyContinue | ForEach-Object { Write-Host '[Locaryn] Releasing port 1420...'; Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }"

if defined WITH_DAEMON (
    set "DAEMON_EXE=%ROOT%\target\debug\locaryn-daemon.exe"
    if not exist "!DAEMON_EXE!" (
        echo [Locaryn] Building daemon... ^(first run can take a few minutes^)
        cargo build -p locaryn-daemon
        if errorlevel 1 exit /b 1
    )
    tasklist /FI "IMAGENAME eq locaryn-daemon.exe" 2>nul | findstr /i "locaryn-daemon.exe" >nul
    if errorlevel 1 (
        echo [Locaryn] Starting daemon on http://127.0.0.1:7474 ...
        start "" /B "!DAEMON_EXE!" > "%ROOT%\tmp\locaryn-daemon.log" 2> "%ROOT%\tmp\locaryn-daemon.err"
        set "DAEMON_STARTED=1"
        ping -n 3 127.0.0.1 >nul
    ) else (
        echo [Locaryn] Daemon already running. Reusing it.
    )
)

set "TAURI_BIN="
if exist "%ROOT%\apps\desktop\node_modules\.bin\tauri.cmd" set "TAURI_BIN=%ROOT%\apps\desktop\node_modules\.bin\tauri.cmd"
if not defined TAURI_BIN if exist "%ROOT%\node_modules\.bin\tauri.cmd" set "TAURI_BIN=%ROOT%\node_modules\.bin\tauri.cmd"
if not defined TAURI_BIN (
    echo [Locaryn] Tauri CLI not found. Running pnpm install...
    pnpm install
    if errorlevel 1 exit /b 1
    if exist "%ROOT%\apps\desktop\node_modules\.bin\tauri.cmd" set "TAURI_BIN=%ROOT%\apps\desktop\node_modules\.bin\tauri.cmd"
)
if not defined TAURI_BIN (
    echo [Locaryn] Tauri CLI still missing. Check apps/desktop/package.json.
    exit /b 1
)

echo [Locaryn] Starting desktop dev ^(tauri dev^)...
pushd "%ROOT%\apps\desktop"
"!TAURI_BIN!" dev
popd

if defined DAEMON_STARTED (
    echo [Locaryn] Stopping daemon...
    taskkill /IM locaryn-daemon.exe /F >nul 2>&1
)

endlocal
exit /b 0
