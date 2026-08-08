# Lochor full release build (Windows).
# Builds all server binaries and the desktop app, then packages artifacts into release/.
#
# Usage:
#   .\scripts\build-all.ps1              # Enterprise remote-server (default)
#   .\scripts\build-all.ps1 -Personal     # Personal (limited) remote-server

param(
    [switch]$Personal,
    [switch]$SkipServers,
    [switch]$SkipDesktop
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path | Join-Path -ChildPath ".." | Resolve-Path
Set-Location $Root

$ReleaseDir = Join-Path $Root "release"
$ServersDir = Join-Path $ReleaseDir "servers"
$DesktopDir = Join-Path $ReleaseDir "desktop"

New-Item -ItemType Directory -Path $ReleaseDir, $ServersDir, $DesktopDir -Force | Out-Null

function Test-Command {
    param([string]$Name)
    return [bool](Get-Command -Name $Name -ErrorAction SilentlyContinue)
}

if (-not (Test-Command -Name "cargo")) {
    Write-Host "[Lochor] cargo not found in PATH. Please install Rust: https://rustup.rs/" -ForegroundColor Red
    exit 1
}
if (-not (Test-Command -Name "pnpm")) {
    Write-Host "[Lochor] pnpm not found in PATH. Please install pnpm (https://pnpm.io/installation)." -ForegroundColor Red
    exit 1
}

$Target = & rustc -vV | Select-String "^host:" | ForEach-Object { ($_ -split "\s+")[1] }
$Variant = if ($Personal) { "personal" } else { "enterprise" }
Write-Host "[Lochor] Building $Variant release for $Target" -ForegroundColor Cyan

if (-not $SkipServers) {
    Write-Host "[Lochor] Building server binaries..." -ForegroundColor Cyan

    cargo build --release -p lochor-cli -p lochor-daemon -p lochor-provider-supervisor
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    if ($Personal) {
        cargo build --release -p lochor-remote-server --no-default-features
    } else {
        cargo build --release -p lochor-remote-server
    }
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host "[Lochor] Copying server binaries..." -ForegroundColor Cyan
    $Binaries = @("lochor.exe", "lochor-daemon.exe", "lochor-remote-server.exe", "lochor-supervisor.exe")
    foreach ($bin in $Binaries) {
        $src = Join-Path $Root "target\release\$bin"
        if (Test-Path $src) {
            Copy-Item $src $ServersDir -Force
        } else {
            Write-Warning "Binary not found: $src"
        }
    }

    $Archive = Join-Path $ReleaseDir "lochor-servers-$Variant-$Target.zip"
    Compress-Archive -Path (Join-Path $ServersDir "*") -DestinationPath $Archive -Force
    Write-Host "[Lochor] Packaged server binaries: $Archive" -ForegroundColor Green
}

if (-not $SkipDesktop) {
    Write-Host "[Lochor] Building desktop app..." -ForegroundColor Cyan
    Push-Location (Join-Path $Root "apps\desktop")
    pnpm install
    if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }
    pnpm tauri build
    if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }
    Pop-Location

    $BundleDir = Join-Path $Root "target\release\bundle"
    if (Test-Path $BundleDir) {
        Copy-Item "$BundleDir\*" $DesktopDir -Recurse -Force
        Write-Host "[Lochor] Desktop bundles copied to $DesktopDir" -ForegroundColor Green
    } else {
        Write-Warning "Desktop bundle directory not found: $BundleDir"
    }
}

Write-Host "[Lochor] Release build complete. Artifacts in $ReleaseDir" -ForegroundColor Green
