# Lochor dev launcher (Windows PowerShell).
#
# The desktop app embeds the Lochor core in-process, so it does NOT need the
# daemon to run. By default this just starts the Tauri desktop dev server.
# Pass -WithDaemon to also run the local daemon (needed only for the CLI).
#
# Usage:
#   .\scripts\dev.ps1                 # launch the desktop app (recommended)
#   .\scripts\dev.ps1 -WithDaemon     # also start the daemon (for CLI testing)
#   .\scripts\dev.ps1 -DaemonOnly     # run only the daemon (foreground)
#   .\scripts\dev.ps1 -SkipBuild      # skip the daemon build when -WithDaemon/-DaemonOnly

param(
    [switch]$WithDaemon,
    [switch]$DaemonOnly,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path | Join-Path -ChildPath ".." | Resolve-Path
Set-Location $Root

function Test-Command {
    param([string]$Name)
    return [bool](Get-Command -Name $Name -ErrorAction SilentlyContinue)
}

function Wait-Health {
    param([int]$MaxSeconds = 15)
    $end = (Get-Date).AddSeconds($MaxSeconds)
    while ((Get-Date) -lt $end) {
        try {
            $null = Invoke-RestMethod -Uri "http://127.0.0.1:7474/health" -TimeoutSec 2
            return $true
        } catch {
            Start-Sleep -Milliseconds 250
        }
    }
    return $false
}

if (-not (Test-Command -Name "cargo")) {
    Write-Host "[Lochor] cargo not found in PATH. Install Rust: https://rustup.rs/" -ForegroundColor Red
    exit 1
}

$tmpDir = Join-Path $Root "tmp"
if (-not (Test-Path $tmpDir)) { New-Item -ItemType Directory -Path $tmpDir | Out-Null }

# --- Optionally start (or run) the daemon ---------------------------------
$daemonProcess = $null
$runDaemon = $WithDaemon -or $DaemonOnly

if ($runDaemon) {
    $daemonExe = Join-Path $Root "target\debug\lochor-daemon.exe"
    if (-not $SkipBuild -and -not (Test-Path $daemonExe)) {
        Write-Host "[Lochor] Building daemon... (first run can take a few minutes)" -ForegroundColor Cyan
        cargo build -p lochor-daemon
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }

    if (Get-Process -Name "lochor-daemon" -ErrorAction SilentlyContinue) {
        Write-Host "[Lochor] Daemon already running. Reusing it." -ForegroundColor Yellow
    } elseif (Test-Path $daemonExe) {
        Write-Host "[Lochor] Starting daemon on http://127.0.0.1:7474 ..." -ForegroundColor Cyan
        $daemonProcess = Start-Process -NoNewWindow -FilePath $daemonExe -PassThru `
            -RedirectStandardOutput (Join-Path $tmpDir "lochor-daemon.log") `
            -RedirectStandardError (Join-Path $tmpDir "lochor-daemon.err")
        if (Wait-Health) {
            Write-Host "[Lochor] Daemon ready (PID $($daemonProcess.Id))." -ForegroundColor Green
        } else {
            # Non-fatal: the desktop app does not need the daemon.
            Write-Host "[Lochor] Daemon health check failed (see tmp\lochor-daemon.log). Continuing." -ForegroundColor Yellow
        }
    } else {
        Write-Host "[Lochor] Daemon binary not found and build was skipped. Continuing without it." -ForegroundColor Yellow
    }

    if ($DaemonOnly) {
        Write-Host "[Lochor] Daemon running. Press Ctrl+C to stop." -ForegroundColor Cyan
        if ($daemonProcess) { Wait-Process -Id $daemonProcess.Id }
        exit 0
    }
}

# --- Launch the desktop app -----------------------------------------------
if (-not (Test-Command -Name "pnpm")) {
    Write-Host "[Lochor] pnpm not found in PATH. Install pnpm: https://pnpm.io/installation" -ForegroundColor Red
    exit 1
}

$portConn = Get-NetTCPConnection -LocalPort 1420 -ErrorAction SilentlyContinue
if ($portConn) {
    Write-Host "[Lochor] Releasing port 1420 (PID $($portConn.OwningProcess))..." -ForegroundColor Yellow
    Stop-Process -Id $portConn.OwningProcess -Force -ErrorAction SilentlyContinue
}

try {
    $desktopTauri = Join-Path $Root "apps\desktop\node_modules\.bin\tauri.cmd"
    $rootTauri = Join-Path $Root "node_modules\.bin\tauri.cmd"
    $tauriBin = if (Test-Path $desktopTauri) { $desktopTauri } elseif (Test-Path $rootTauri) { $rootTauri } else { $null }

    if (-not $tauriBin) {
        Write-Host "[Lochor] Tauri CLI not found. Running pnpm install..." -ForegroundColor Yellow
        pnpm install
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        $tauriBin = if (Test-Path $desktopTauri) { $desktopTauri } elseif (Test-Path $rootTauri) { $rootTauri } else { $null }
        if (-not $tauriBin) {
            Write-Host "[Lochor] Tauri CLI still missing after install. Check apps/desktop/package.json." -ForegroundColor Red
            exit 1
        }
    }

    Write-Host "[Lochor] Starting desktop dev (tauri dev)..." -ForegroundColor Cyan
    Push-Location (Join-Path $Root "apps\desktop")
    & $tauriBin dev
    Pop-Location
} finally {
    if ($daemonProcess) {
        Write-Host "[Lochor] Stopping daemon (PID $($daemonProcess.Id))..." -ForegroundColor Cyan
        Stop-Process -Id $daemonProcess.Id -Force -ErrorAction SilentlyContinue
    }
}
