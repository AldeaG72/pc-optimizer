<#
.SYNOPSIS
  Downloads the official PresentMon console app (Intel, MIT license) used by `pco bench`,
  and refuses it unless its SHA-256 matches the pinned value.

  Source: https://github.com/GameTechDev/PresentMon/releases (release v2.6.0).
  The file is NOT committed to git: it goes to vendor\presentmon\ (ignored).
  Keep VERSION/FILE/SHA256 in sync with crates/core/src/bench/presentmon.rs (a test checks).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\fetch-presentmon.ps1
#>
param(
    [string]$Dest = (Join-Path $PSScriptRoot '..\vendor\presentmon')
)

$ErrorActionPreference = 'Stop'
$VERSION = '2.6.0'
$FILE = 'PresentMon-2.6.0-x64.exe'
$SHA256 = 'b2a706bc6ad475749e3b7e3409263aa1e6906d45bdcf993f6dbc0f660188f1af'
$URL = "https://github.com/GameTechDev/PresentMon/releases/download/v$VERSION/$FILE"

New-Item -ItemType Directory -Force -Path $Dest | Out-Null
$target = Join-Path $Dest $FILE

if (Test-Path $target) {
    $have = (Get-FileHash -Algorithm SHA256 -Path $target).Hash.ToLowerInvariant()
    if ($have -eq $SHA256) {
        Write-Host "PresentMon $VERSION already present and verified: $target"
        exit 0
    }
    Remove-Item -Force $target
}

$tmp = "$target.download"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Invoke-WebRequest -UseBasicParsing -Uri $URL -OutFile $tmp
$got = (Get-FileHash -Algorithm SHA256 -Path $tmp).Hash.ToLowerInvariant()
if ($got -ne $SHA256) {
    Remove-Item -Force $tmp
    Write-Host "REFUSED: SHA-256 mismatch for $FILE (expected $SHA256, got $got)." -ForegroundColor Red
    exit 2
}
Move-Item -Force $tmp $target
Write-Host "PresentMon $VERSION downloaded and verified: $target" -ForegroundColor Green
exit 0
