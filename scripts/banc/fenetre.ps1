# Trouve l'application du banc (par le dossier d'où elle a été lancée), écrit
# son identifiant de processus et pose sa fenêtre hors de l'écran.
#
# Hors écran, pas réduite : une fenêtre réduite cesse de se dessiner, et les
# captures seraient vides.
#
# Avec -Arreter : arrête l'application du banc et ce qu'elle a lancé, et rien
# d'autre — jamais l'application de la personne, lancée d'un autre dossier.
param(
    [Parameter(Mandatory = $true)][string]$DossierApp,
    [string]$FichierPid = '',
    [switch]$Arreter
)

$ErrorActionPreference = 'Stop'
$racine = (Resolve-Path $DossierApp).Path

$du_banc = @(Get-CimInstance Win32_Process -Filter "Name='locaryn-desktop.exe'" |
    Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($racine, [System.StringComparison]::OrdinalIgnoreCase) })

if ($Arreter) {
    foreach ($p in $du_banc) {
        & taskkill /F /T /PID $p.ProcessId | Out-Null
    }
    exit 0
}

$processus = $du_banc | Select-Object -First 1
if (-not $processus) {
    Write-Error "application du banc introuvable dans $racine"
    exit 1
}
Set-Content -Path $FichierPid -Value $processus.ProcessId -Encoding ascii

Add-Type -Namespace Banc -Name Fenetre -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr apres, int x, int y, int cx, int cy, uint drapeaux);
'@

# La fenêtre principale n'existe qu'une fois l'écran de lancement passé.
$SANS_ACTIVER_NI_ORDONNER = 0x0014
for ($essai = 0; $essai -lt 40; $essai++) {
    $p = Get-Process -Id $processus.ProcessId -ErrorAction SilentlyContinue
    if ($p -and $p.MainWindowHandle -ne 0) {
        [Banc.Fenetre]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, -4000, 0, 1400, 900, $SANS_ACTIVER_NI_ORDONNER) | Out-Null
        Write-Output $processus.ProcessId
        exit 0
    }
    Start-Sleep -Milliseconds 500
}
Write-Error "la fenêtre de l'application n'est pas apparue"
exit 1
