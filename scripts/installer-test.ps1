<#
.SYNOPSIS
  Installer test (Stage 3: 3.15), GitHub Actions only: install silently, apply tweaks with
  the installed `pco`, uninstall silently, then check every target is exactly as before.

  CLAUDE.md safety rule 1: it changes real settings, so it refuses to run outside GitHub
  Actions (GITHUB_ACTIONS=true and PCO_CI_REAL_TWEAKS=1).
#>
param([Parameter(Mandatory)][string]$Setup)

$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:PCO_CI_REAL_TWEAKS -ne '1') {
    throw 'refusing to change real Windows settings outside GitHub Actions (CLAUDE.md, rule 1)'
}

# Recommended, registry-only recipes (HKCU and HKLM), so the comparison is exact.
$ids = @(
    'gaming.game-dvr-off', 'gaming.game-mode-on', 'privacy.advertising-id-off',
    'privacy.telemetry-minimum', 'privacy.activity-history-off', 'debloat.widgets-off',
    'debloat.edge-background-off', 'windows.taskbar-end-task'
)
$repoPco = Join-Path $PSScriptRoot '..\target\release\pco.exe'

function Get-State {
    # The repo's own pco (not the installed one), so it still exists after uninstalling.
    $out = & $repoPco --lang en plan @ids
    if ($LASTEXITCODE -ne 0) { throw "pco plan failed: $out" }
    ($out | Out-String)
}

$before = Get-State

Write-Host "Installing $Setup"
$p = Start-Process -FilePath $Setup -ArgumentList '/S' -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "installer exit $($p.ExitCode)" }
$dir = Join-Path $env:ProgramFiles 'PC Optimizer'
foreach ($f in 'pc-optimizer.exe', 'pco-helper.exe', 'pco.exe', 'presentmon\PresentMon-2.6.0-x64.exe', 'uninstall.exe') {
    if (-not (Test-Path (Join-Path $dir $f))) { throw "missing after install: $f" }
}
Write-Host '::notice title=Installer::installed in Program Files with the helper, pco and PresentMon'

& (Join-Path $dir 'pco.exe') --lang en apply @ids
if ($LASTEXITCODE -ne 0) { throw "installed pco apply failed ($LASTEXITCODE)" }
$applied = Get-State
if ($applied -eq $before) { throw 'nothing changed after apply' }

Write-Host 'Uninstalling'
# _?= keeps the uninstaller in place, so Start-Process waits for it.
$p = Start-Process -FilePath (Join-Path $dir 'uninstall.exe') -ArgumentList '/S', "_?=$dir" -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "uninstaller exit $($p.ExitCode)" }
if (Test-Path (Join-Path $dir 'pc-optimizer.exe')) { throw 'the app is still there after uninstalling' }

$after = Get-State
if ($after -ne $before) {
    Write-Host "BEFORE:`n$before`nAFTER:`n$after"
    throw 'the uninstaller did not restore every setting exactly'
}
Write-Host '::notice title=Installer::install → apply 8 recommended tweaks → uninstall: every setting exactly as before'
