# Lochor clean script (Windows).
# Removes release/, target/, node_modules, and Tauri bundle outputs.

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path | Join-Path -ChildPath ".." | Resolve-Path
Set-Location $Root

Write-Host "[Lochor] Cleaning build artifacts..." -ForegroundColor Cyan

$Dirs = @(
    "release",
    "target",
    "apps\desktop\node_modules",
    "apps\desktop\dist",
    "packages-ui\core\node_modules",
    "packages-ui\chat\node_modules",
    "packages-ui\preview\node_modules",
    "packages-ui\terminal\node_modules",
    "node_modules"
)

foreach ($dir in $Dirs) {
    $path = Join-Path $Root $dir
    if (Test-Path $path) {
        Remove-Item $path -Recurse -Force
        Write-Host "  Removed $dir" -ForegroundColor DarkGray
    }
}

Write-Host "[Lochor] Clean complete." -ForegroundColor Green
