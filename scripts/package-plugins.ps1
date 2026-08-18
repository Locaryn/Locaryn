# Package each extension in plugins/ into a standardized .zip archive in release/plugins/
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path | Join-Path -ChildPath ".." | Resolve-Path
Set-Location $Root

$PluginsDir = Join-Path $Root "plugins"
$ReleasePluginsDir = Join-Path $Root "release\plugins"

New-Item -ItemType Directory -Path $ReleasePluginsDir -Force | Out-Null

Write-Host "[Locaryn] Packaging extensions from $PluginsDir..." -ForegroundColor Cyan

$PluginFolders = Get-ChildItem -Path $PluginsDir -Directory

foreach ($folder in $PluginFolders) {
    $manifestPath = Join-Path $folder.FullName "plugin.json"
    if (Test-Path $manifestPath) {
        $zipName = "$($folder.Name).zip"
        $destination = Join-Path $ReleasePluginsDir $zipName
        Write-Host "  -> Packaging $($folder.Name) into $zipName" -ForegroundColor Yellow
        if (Test-Path $destination) {
            Remove-Item $destination -Force
        }
        Compress-Archive -Path (Join-Path $folder.FullName "*") -DestinationPath $destination -Force
    }
}

Write-Host "[Locaryn] All extensions successfully packaged to $ReleasePluginsDir" -ForegroundColor Green
