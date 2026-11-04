# Envoie le paquet MSIX au Microsoft Store et le soumet à la certification (CI, sous Windows).
# Prérequis : la CLI msstore installée et configurée (`msstore reconfigure`).
#
#   pwsh scripts/store-publish.ps1 -Package target/msix/GeoLearner_0.3.0.0_x64.msix
#
# Étapes : brouillon de soumission avec le nouveau paquet (`publish --noCommit`), notes de version
# de chaque langue reprises de packaging/windows/release-notes/<langue>.txt, puis soumission.
# Les autres champs de la fiche (textes, captures, mots-clés) restent ceux de la version en ligne.
param(
    [Parameter(Mandatory)] [string] $Package,
    [string] $ProductId = "9N1C5KFNMV7H"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# Version x.y.z du workspace : les notes de version doivent mentionner « x.y », sinon ce sont
# celles de la version précédente, oubliées.
$cargo = Get-Content Cargo.toml -Raw
if ($cargo -notmatch '(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"') { throw "version introuvable dans Cargo.toml" }
$short = "$($Matches[1]).$($Matches[2])"
$notes = @{}
foreach ($file in Get-ChildItem packaging/windows/release-notes/*.txt) {
    $text = (Get-Content $file -Raw -Encoding utf8).Trim()
    if ($text -notmatch [regex]::Escape($short)) {
        throw "$($file.Name) ne mentionne pas la version $short : notes de version à mettre à jour"
    }
    $notes[$file.BaseName] = $text
}

function Invoke-Store {
    & msstore @args
    if ($LASTEXITCODE -ne 0) { throw "msstore $($args[0]) a échoué ($LASTEXITCODE)" }
}

Write-Host "Brouillon de soumission avec $Package"
Invoke-Store publish $Package --appId $ProductId --noCommit

$json = (& msstore submission get $ProductId) -join "`n"
if ($LASTEXITCODE -ne 0) { throw "msstore submission get a échoué ($LASTEXITCODE)" }
$submission = $json | ConvertFrom-Json
foreach ($lang in $notes.Keys) {
    $listing = $submission.Listings.$lang
    if (-not $listing) { throw "fiche $lang absente de la soumission" }
    $listing.BaseListing.ReleaseNotes = $notes[$lang]
    Write-Host "Notes de version $lang mises à jour"
}
$metadata = $submission | ConvertTo-Json -Depth 64 -Compress
Invoke-Store submission updateMetadata $ProductId $metadata

Write-Host "Soumission à la certification"
Invoke-Store submission publish $ProductId
Invoke-Store submission status $ProductId
