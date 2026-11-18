# Populates winget/templates with a published Cosmic Font Foundry release,
# downloads the installer, computes SHA-256, and writes ready-to-submit manifests.
#
#   ./winget/prepare.ps1 0.3.0
param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = "Stop"
$v = $Version.TrimStart("v")
$url = "https://github.com/theCosmicCrafter/tuna-fonts/releases/download/v$v/Cosmic-Font-Foundry_${v}_x64-setup.exe"
$tmp = Join-Path ([IO.Path]::GetTempPath()) "Cosmic-Font-Foundry_${v}_x64-setup.exe"
Invoke-WebRequest $url -OutFile $tmp
$sha = (Get-FileHash $tmp -Algorithm SHA256).Hash.ToUpper()
Remove-Item $tmp
$date = ([datetime](Invoke-RestMethod "https://api.github.com/repos/theCosmicCrafter/tuna-fonts/releases/tags/v$v").published_at).ToString("yyyy-MM-dd")
$outDir = Join-Path $PSScriptRoot "manifests/t/TheCosmicCrafter/CosmicFontFoundry/$v"
New-Item -ItemType Directory -Force $outDir | Out-Null
foreach ($f in Get-ChildItem (Join-Path $PSScriptRoot "templates") -Filter *.yaml) {
  $text = (Get-Content $f.FullName -Raw).Replace("{VERSION}", $v).Replace("{SHA256}", $sha).Replace("{DATE}", $date)
  [IO.File]::WriteAllText((Join-Path $outDir $f.Name), $text)
}
"SHA-256: $sha"
"Manifests written to: $outDir"
"Next: winget validate --manifest `"$outDir`""
