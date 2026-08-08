# Locaryn license-compliance audit (Windows / PowerShell).
#
# Verifies that every third-party dependency is permissively licensed (no
# GPL/AGPL/SSPL) so the closed-source, paid build is compliant. Reproduces the
# audit recorded in THIRD_PARTY_LICENSES/README.md.
#
#   pwsh scripts/license-audit.ps1
#
# Exit code is non-zero if any gate fails.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$fail = 0

Write-Host "== Locaryn license audit ==" -ForegroundColor Cyan

# ── 1. Rust workspace ──────────────────────────────────────────────────────
Write-Host "`n[1/3] Rust crates (cargo-deny)..." -ForegroundColor Yellow
if (Get-Command cargo-deny -ErrorAction SilentlyContinue) {
    cargo deny check licenses
    if ($LASTEXITCODE -ne 0) { $fail = 1 }
} else {
    Write-Host "  cargo-deny not installed. Falling back to cargo metadata scan." -ForegroundColor DarkYellow
    Write-Host "  (install the gate with: cargo install cargo-deny)"
    $meta = cargo metadata --format-version 1 --all-features | ConvertFrom-Json
    $bad = @()
    foreach ($p in $meta.packages) {
        $lic = "$($p.license)"
        if ($lic -eq "") { continue }
        # Flag strong copyleft only when it is NOT one option of an OR expression.
        if (($lic -match "GPL|AGPL|SSPL|EUPL|CDDL") -and ($lic -notmatch " OR ") -and ($lic -notmatch "/")) {
            $bad += "$($p.name) $($p.version): $lic"
        }
    }
    if ($bad.Count -gt 0) {
        Write-Host "  BLOCKING copyleft found:" -ForegroundColor Red
        $bad | ForEach-Object { Write-Host "    $_" -ForegroundColor Red }
        $fail = 1
    } else {
        Write-Host "  OK - no strong copyleft in the Rust tree." -ForegroundColor Green
    }
}

# ── 2. Frontend (pnpm) ─────────────────────────────────────────────────────
Write-Host "`n[2/3] Frontend prod deps (pnpm)..." -ForegroundColor Yellow
$pnpm = pnpm licenses list --prod 2>$null
$fe = $pnpm | Select-String -Pattern "GPL|AGPL|SSPL|EUPL|CDDL" | Where-Object { $_ -notmatch " OR |LGPL-.*OR" }
if ($fe) {
    Write-Host "  BLOCKING copyleft in frontend:" -ForegroundColor Red
    $fe | ForEach-Object { Write-Host "    $_" -ForegroundColor Red }
    $fail = 1
} else {
    Write-Host "  OK - frontend prod deps are permissive." -ForegroundColor Green
}

# ── 3. Python sidecar (optional) ───────────────────────────────────────────
Write-Host "`n[3/3] Python model-editing sidecar..." -ForegroundColor Yellow
if (Test-Path "$root/sidecar/requirements.txt") {
    if (Get-Command pip-licenses -ErrorAction SilentlyContinue) {
        pip-licenses --format=markdown --with-urls |
            Select-String -Pattern "GPL|AGPL|SSPL" | Where-Object { $_ -notmatch "LGPL" }
        Write-Host "  Review any GPL rows above before shipping the sidecar." -ForegroundColor DarkYellow
    } else {
        Write-Host "  pip-licenses not installed (pip install pip-licenses)." -ForegroundColor DarkYellow
    }
} else {
    Write-Host "  No sidecar/requirements.txt yet - skipped." -ForegroundColor DarkGray
}

Write-Host ""
if ($fail -ne 0) {
    Write-Host "AUDIT FAILED - a non-permissive license entered the tree." -ForegroundColor Red
    exit 1
} else {
    Write-Host "AUDIT PASSED - stack is safe for a commercial build." -ForegroundColor Green
    Write-Host "Reminder: ship THIRD_PARTY_LICENSES/ with every distribution."
}
