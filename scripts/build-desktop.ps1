# Locaryn desktop app release build (Windows).
# Builds the Tauri desktop app and copies bundles to release/desktop/.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path | Join-Path -ChildPath ".." | Resolve-Path
Set-Location $Root

$ReleaseDir = Join-Path $Root "release"
$DesktopDir = Join-Path $ReleaseDir "desktop"

New-Item -ItemType Directory -Path $DesktopDir -Force | Out-Null

if (-not (Get-Command -Name "pnpm" -ErrorAction SilentlyContinue)) {
    Write-Host "[Locaryn] pnpm not found in PATH. Please install pnpm (https://pnpm.io/installation)." -ForegroundColor Red
    exit 1
}

Write-Host "[Locaryn] Building locaryn-daemon and locaryn CLI in release mode..." -ForegroundColor Cyan
cargo build --release -p locaryn-daemon -p locaryn-cli
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$BinariesDir = Join-Path $Root "apps\desktop\src-tauri\binaries"
$ServersDir = Join-Path $ReleaseDir "servers"
New-Item -ItemType Directory -Path $BinariesDir -Force | Out-Null
New-Item -ItemType Directory -Path $ServersDir -Force | Out-Null

Copy-Item (Join-Path $Root "target\release\locaryn-daemon.exe") (Join-Path $BinariesDir "locaryn-daemon-x86_64-pc-windows-msvc.exe") -Force
Copy-Item (Join-Path $Root "target\release\locaryn.exe") (Join-Path $BinariesDir "locaryn-x86_64-pc-windows-msvc.exe") -Force
Copy-Item (Join-Path $Root "target\release\locaryn-daemon.exe") (Join-Path $ServersDir "locaryn-daemon.exe") -Force
Copy-Item (Join-Path $Root "target\release\locaryn.exe") (Join-Path $ServersDir "locaryn.exe") -Force

Write-Host "[Locaryn] Building desktop app for Windows..." -ForegroundColor Cyan

Push-Location (Join-Path $Root "apps\desktop")
pnpm install
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }
pnpm tauri build --no-sign
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }
Pop-Location

$BundleDir = Join-Path $Root "target\release\bundle"
if (Test-Path $BundleDir) {
    Copy-Item "$BundleDir\*" $DesktopDir -Recurse -Force
    Write-Host "[Locaryn] Desktop bundles copied to $DesktopDir" -ForegroundColor Green
} else {
    Write-Warning "Desktop bundle directory not found: $BundleDir"
}
