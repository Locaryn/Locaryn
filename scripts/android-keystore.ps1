# Crée la clé de signature Android du projet, puis affiche ce qu'il faut
# déposer dans les secrets GitHub.
#
# Pourquoi un script plutôt qu'une clé dans le dépôt : cette clé fixe
# l'identité de l'application pour toutes ses mises à jour. Android refuse une
# mise à jour signée par une autre clé — la perdre oblige à republier sous un
# autre nom de paquet, et la partager revient à laisser n'importe qui publier
# « Locaryn ». Elle appartient au propriétaire du projet, pas à la CI, et pas
# au dépôt.
#
#   pwsh scripts/android-keystore.ps1
#
# Le fichier produit est ignoré par git (.android-keys/), et la sortie donne
# les quatre commandes `gh secret set` à lancer.

param(
    [string] $Out = ".android-keys/locaryn-release.jks",
    [string] $Alias = "locaryn",
    [int]    $ValidityDays = 10950  # 30 ans : au-delà de la vie du magasin.
)

$ErrorActionPreference = "Stop"

$keytool = (Get-Command keytool -ErrorAction SilentlyContinue).Source
if (-not $keytool) {
    $candidates = Get-ChildItem "C:\Program Files\*\*\bin\keytool.exe" -ErrorAction SilentlyContinue
    if ($candidates) { $keytool = $candidates[0].FullName }
}
if (-not $keytool) {
    throw "keytool est introuvable. Installez un JDK (Temurin 17 ou plus), puis relancez."
}

if (Test-Path $Out) {
    throw "$Out existe déjà. Signez avec cette clé, ou déplacez-la avant d'en créer une autre : deux clés veulent dire deux applications différentes pour Android."
}

$dir = Split-Path -Parent $Out
if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }

$password = Read-Host "Mot de passe de la clé (gardez-le : il est aussi indispensable que le fichier)" -AsSecureString
$plain = [Runtime.InteropServices.Marshal]::PtrToStringAuto(
    [Runtime.InteropServices.Marshal]::SecureStringToBSTR($password))
if ($plain.Length -lt 6) { throw "keytool exige au moins 6 caractères." }

& $keytool -genkeypair -v `
    -keystore $Out `
    -alias $Alias `
    -keyalg RSA -keysize 4096 `
    -validity $ValidityDays `
    -storepass $plain -keypass $plain `
    -dname "CN=Locaryn, OU=Locaryn, O=Locaryn, C=FR"
if ($LASTEXITCODE -ne 0) { throw "keytool a échoué ($LASTEXITCODE)." }

$b64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($Out))
$b64Path = "$Out.base64"
Set-Content -Path $b64Path -Value $b64 -Encoding ascii -NoNewline

Write-Host ""
Write-Host "Clé créée : $Out"
Write-Host "Sauvegardez ce fichier hors de la machine. Sans lui, plus aucune mise à jour de l'application publiée n'est possible."
Write-Host ""
Write-Host "Déposez ensuite les quatre secrets (le contenu base64 est dans $b64Path) :"
Write-Host ""
Write-Host "  gh secret set ANDROID_KEYSTORE_BASE64 --repo Locaryn/Locaryn < `"$b64Path`""
Write-Host "  gh secret set ANDROID_KEYSTORE_PASSWORD --repo Locaryn/Locaryn"
Write-Host "  gh secret set ANDROID_KEY_ALIAS --repo Locaryn/Locaryn      # valeur : $Alias"
Write-Host "  gh secret set ANDROID_KEY_PASSWORD --repo Locaryn/Locaryn"
Write-Host ""
Write-Host "La prochaine release publiera alors un APK signé, installable tel quel."
