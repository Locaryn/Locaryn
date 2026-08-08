# Locaryn server binaries release build (Windows).
# Builds CLI, daemon, remote-server, and provider-supervisor in release mode.
#
# Usage:
#   .\scripts\build-servers.ps1              # Enterprise remote-server (default)
#   .\scripts\build-servers.ps1 -Personal     # Personal (limited) remote-server

param(
    [switch]$Personal
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path | Join-Path -ChildPath ".." | Resolve-Path
Set-Location $Root

$ReleaseDir = Join-Path $Root "release"
$ServersDir = Join-Path $ReleaseDir "servers"

New-Item -ItemType Directory -Path $ServersDir -Force | Out-Null

if (-not (Get-Command -Name "cargo" -ErrorAction SilentlyContinue)) {
    Write-Host "[Locaryn] cargo not found in PATH. Please install Rust: https://rustup.rs/" -ForegroundColor Red
    exit 1
}

$Target = & rustc -vV | Select-String "^host:" | ForEach-Object { ($_ -split "\s+")[1] }
$Variant = if ($Personal) { "personal" } else { "enterprise" }
Write-Host "[Locaryn] Building $Variant server binaries for $Target" -ForegroundColor Cyan

$RemoteServerFeatures = if ($Personal) { "" } else { "--features=enterprise" }

# Build common binaries.
cargo build --release -p locaryn-cli -p locaryn-daemon -p locaryn-provider-supervisor
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Build remote-server with the chosen feature set.
if ($Personal) {
    cargo build --release -p locaryn-remote-server --no-default-features
} else {
    cargo build --release -p locaryn-remote-server
}
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$Binaries = @("locaryn.exe", "locaryn-daemon.exe", "locaryn-remote-server.exe", "locaryn-supervisor.exe")
foreach ($bin in $Binaries) {
    $src = Join-Path $Root "target\release\$bin"
    if (Test-Path $src) {
        Copy-Item $src $ServersDir -Force
    } else {
        Write-Warning "Binary not found: $src"
    }
}

$Archive = Join-Path $ReleaseDir "locaryn-servers-$Variant-$Target.zip"
Compress-Archive -Path (Join-Path $ServersDir "*") -DestinationPath $Archive -Force

Write-Host "[Locaryn] Server binaries built and packaged:" -ForegroundColor Green
Write-Host "  Directory: $ServersDir" -ForegroundColor Green
Write-Host "  Archive:   $Archive" -ForegroundColor Green
