# Construit le paquet MSIX à soumettre au Microsoft Store (à lancer sous Windows, après
# `cargo build --release -p geo-learner`). Nécessite le SDK Windows (makeappx.exe).
#
#   pwsh scripts/package-msix.ps1
#
# Le paquet n'est pas signé : le Store le signe lui-même à la publication.
# Pour essayer l'application empaquetée sans la publier (mode développeur activé) :
#   Add-AppxPackage -Register target\msix\layout\AppxManifest.xml

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$store = Get-Content packaging/windows/store.json -Raw | ConvertFrom-Json

# Version du workspace (x.y.z) → version MSIX (x.y.z.0).
$cargo = Get-Content Cargo.toml -Raw
if ($cargo -notmatch '(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"') { throw "version introuvable dans Cargo.toml" }
$version = "$($Matches[1]).$($Matches[2]).$($Matches[3]).0"

$exe = "target/release/geo-learner.exe"
if (-not (Test-Path $exe)) { throw "$exe introuvable : lancer d'abord cargo build --release -p geo-learner" }

$out = "target/msix"
$layout = "$out/layout"
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$layout/Assets" | Out-Null
Copy-Item $exe $layout
Copy-Item packaging/windows/Assets/*.png "$layout/Assets"

$manifest = (Get-Content packaging/windows/AppxManifest.xml -Raw).
    Replace("{{IDENTITY_NAME}}", $store.identityName).
    Replace("{{PUBLISHER}}", $store.publisher).
    Replace("{{PUBLISHER_DISPLAY_NAME}}", $store.publisherDisplayName).
    Replace("{{DISPLAY_NAME}}", $store.displayName).
    Replace("{{VERSION}}", $version)
Set-Content -Path "$layout/AppxManifest.xml" -Value $manifest -Encoding utf8

$makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $makeappx) { throw "makeappx.exe introuvable : installer le SDK Windows" }

$package = "$out/GeoLearner_${version}_x64.msix"
& $makeappx.FullName pack /o /d $layout /p $package
if ($LASTEXITCODE -ne 0) { throw "makeappx a échoué" }
Write-Host "Paquet : $package"
