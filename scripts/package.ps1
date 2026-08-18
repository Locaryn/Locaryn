# Locaryn packaging helper (Windows).
# Assumes build scripts have already produced binaries. Packages them into release/.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path | Join-Path -ChildPath ".." | Resolve-Path
Set-Location $Root

$ReleaseDir = Join-Path $Root "release"
$ServersDir = Join-Path $ReleaseDir "servers"
$DesktopDir = Join-Path $ReleaseDir "desktop"

New-Item -ItemType Directory -Path $ServersDir, $DesktopDir -Force | Out-Null

$Target = & rustc -vV | Select-String "^host:" | ForEach-Object { ($_ -split "\s+")[1] }

Write-Host "[Locaryn] Packaging artifacts for $Target" -ForegroundColor Cyan

# Package server binaries
$Binaries = @("locaryn.exe", "locaryn-daemon.exe", "locaryn-remote-server.exe", "locaryn-supervisor.exe")
$Found = $false
foreach ($bin in $Binaries) {
    $src = Join-Path $Root "target\release\$bin"
    if (Test-Path $src) {
        Copy-Item $src $ServersDir -Force
        $Found = $true
    }
}

if ($Found) {
    $Archive = Join-Path $ReleaseDir "locaryn-servers-$Target.zip"
    Compress-Archive -Path (Join-Path $ServersDir "*") -DestinationPath $Archive -Force
    Write-Host "[Locaryn] Packaged server binaries: $Archive" -ForegroundColor Green
} else {
    Write-Warning "No server binaries found in target\release. Run build-servers.ps1 first."
}

# Package desktop bundles
$BundleDir = Join-Path $Root "target\release\bundle"
if (Test-Path $BundleDir) {
    Copy-Item "$BundleDir\*" $DesktopDir -Recurse -Force
    Write-Host "[Locaryn] Desktop bundles copied to $DesktopDir" -ForegroundColor Green
} else {
    Write-Warning "No desktop bundles found. Run build-desktop.ps1 first."
}

# Package extensions
& (Join-Path $Root "scripts\package-plugins.ps1")

