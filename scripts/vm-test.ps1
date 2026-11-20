<#
.SYNOPSIS
  Runs the Stage 0 VM tests (docs/TESTARE-ETAPA-0.md, tests A-D) automatically and writes a
  report. ONLY for a throwaway Windows: a VM, Windows Sandbox or a CI runner — it applies
  real tweaks (and undoes them). Never run it on the development PC (CLAUDE.md, rule 1).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File C:\pco\vm-test.ps1
#>
param(
    [string]$Pco = (Join-Path $PSScriptRoot 'pco.exe'),
    [string]$Snapshot = (Join-Path $PSScriptRoot 'vm-snapshot.ps1'),
    [string]$Work = (Join-Path $env:TEMP 'pco-vm-test'),
    # Required, so nobody runs it by accident.
    [switch]$IAmInAThrowawayWindows
)

if (-not $IAmInAThrowawayWindows) {
    Write-Host 'This script changes real Windows settings. Run it only in a VM, Windows Sandbox or CI,'
    Write-Host 'and confirm with: -IAmInAThrowawayWindows'
    exit 3
}

# pco writes UTF-8; read it as UTF-8 (Romanian diacritics).
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$ErrorActionPreference = 'Stop'
$onGitHub = $env:GITHUB_ACTIONS -eq 'true'

Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Work | Out-Null
$env:PCO_DATA_DIR = Join-Path $Work 'data'
$report = New-Object System.Collections.Generic.List[object]
$log = Join-Path $Work 'log.txt'

function Log([string]$text) { Add-Content -Encoding UTF8 -Path $log -Value $text }

function Check([string]$test, [string]$what, [bool]$ok, [string]$detail = '') {
    $status = if ($ok) { 'OK' } else { 'FAIL' }
    $report.Add([pscustomobject]@{ Test = $test; Check = $what; Result = $status; Detail = $detail })
    $color = if ($ok) { 'Green' } else { 'Red' }
    Write-Host ("[{0}] {1} {2} {3}" -f $status, $test, $what, $detail) -ForegroundColor $color
    if ($onGitHub) {
        $level = if ($ok) { 'notice' } else { 'error' }
        $msg = ("$what $detail").Replace("`r", ' ').Replace("`n", ' | ')
        Write-Host "::$level title=VM test ${test}::$msg"
    }
}

function Skip([string]$test, [string]$what, [string]$why) {
    $report.Add([pscustomobject]@{ Test = $test; Check = $what; Result = 'SKIP'; Detail = $why })
    Write-Host "[SKIP] $test $what ($why)" -ForegroundColor Yellow
}

function Pco {
    $out = (& $Pco @args 2>&1 | Out-String).Trim()
    $code = $LASTEXITCODE
    Log "> pco $($args -join ' ')  (exit $code)"
    Log $out
    return [pscustomobject]@{ Out = $out; Code = $code }
}

function Snap([string]$name, [string]$compareWith = '') {
    $target = Join-Path $Work $name
    $argv = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $Snapshot, '-Out', $target)
    if ($compareWith) { $argv += @('-CompareWith', (Join-Path $Work $compareWith)) }
    $out = (& powershell.exe @argv 2>&1 | Out-String).Trim()
    $code = $LASTEXITCODE
    Log "> snapshot $name $compareWith (exit $code)"
    Log $out
    return [pscustomobject]@{ Out = $out; Code = $code }
}

$elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
$build = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').CurrentBuildNumber
Write-Host "Windows build $build, administrator: $elevated"
Log "Windows build $build, administrator: $elevated"

$all = @('gaming.game-mode-on', 'gaming.game-dvr-off', 'input.sticky-keys-hotkey-off',
         'privacy.advertising-id-off', 'windows.taskbar-end-task', 'services.fax-disabled')
$registryRecipes = $all[0..4]

# --- A: read only ------------------------------------------------------------------------
$r = Pco --version
Check 'A' '--version' ($r.Code -eq 0 -and $r.Out -match '^pco \d') $r.Out

$before = Snap 'a-before'
$r = Pco --lang ro list
$listOk = $r.Code -eq 0
foreach ($id in $all) { $listOk = $listOk -and $r.Out.Contains($id) }
Check 'A' 'list shows all 6 recipes' $listOk

$r = Pco --lang ro plan gaming.game-mode-on input.sticky-keys-hotkey-off
Check 'A' 'plan shows old → new' ($r.Code -eq 0 -and ($r.Out.Contains('→') -or $r.Out.Contains('deja așa')))

$r = Pco --lang ro apply gaming.game-dvr-off --dry-run
Check 'A' 'dry run says nothing changed' ($r.Code -eq 0 -and $r.Out.Contains('Simulare: nu am schimbat nimic.'))

$r = Pco --lang ro status
Check 'A' 'status on an empty journal' ($r.Code -eq 0 -and $r.Out.Contains('Nicio sesiune în jurnal.'))

$s = Snap 'a-after' 'a-before'
Check 'A' 'read-only commands changed nothing' ($s.Code -eq 0)

# --- D2: without administrator rights (only when not elevated) ---------------------------
if (-not $elevated) {
    $r = Pco --lang ro apply privacy.advertising-id-off
    Check 'D2' 'admin needed → blocked (exit 2)' ($r.Code -eq 2 -and $r.Out.Contains('drepturi de administrator')) "exit $($r.Code)"
    Skip 'B/C/D1' 'apply/undo tests' 'need an administrator window'
} else {
    Skip 'D2' 'blocked without admin' 'this window is administrator; covered by automated tests'

    # --- B: apply everything, undo everything, compare -----------------------------------
    $s = Snap 'b-before'
    $r = Pco --lang ro apply @all
    Check 'B' 'apply exits 0' ($r.Code -eq 0) "exit $($r.Code)"
    foreach ($id in $registryRecipes) {
        $line = ($r.Out -split "`r?`n") | Where-Object { $_ -match [regex]::Escape(" ${id}:") } | Select-Object -First 1
        $ok = $line -match '^\+ .*aplicat|^= .*era deja aplicat'
        if (-not $ok -and $id -eq 'input.sticky-keys-hotkey-off' -and $line -match '^- ') {
            Skip 'B' "apply $id" "Sticky Keys was not the Windows default here: $line"
        } else {
            Check 'B' "apply $id" ([bool]$ok) "$line"
        }
    }
    $r = Pco --lang ro list
    Check 'B' 'list after apply shows "aplicat"' ($r.Code -eq 0 -and $r.Out -match '\saplicat')

    $r = Pco --lang ro undo --all
    Check 'B' 'undo --all exits 0' ($r.Code -eq 0) "exit $($r.Code)"
    $s = Snap 'b-after' 'b-before'
    Check 'B' 'registry exactly as before after undo' ($s.Code -eq 0) ($s.Out -split "`r?`n" | Select-Object -Last 3 | Out-String)

    # --- C: interrupted apply ------------------------------------------------------------
    # Make sure the first recipe has something to change, remembering the original value.
    $gameBar = 'HKCU:\Software\Microsoft\GameBar'
    $orig = Get-ItemProperty -Path $gameBar -Name AutoGameModeEnabled -ErrorAction SilentlyContinue
    $keyExisted = Test-Path $gameBar
    if (-not $keyExisted) { New-Item -Path $gameBar -Force | Out-Null }
    Set-ItemProperty -Path $gameBar -Name AutoGameModeEnabled -Value 0 -Type DWord

    $s = Snap 'c-before'
    $env:PCO_TEST_STOP_AFTER_CHANGES = '1'
    $r = Pco --lang ro apply gaming.game-mode-on gaming.game-dvr-off privacy.advertising-id-off
    Remove-Item Env:PCO_TEST_STOP_AFTER_CHANGES
    Check 'C' 'pco stops after one change (exit 99)' ($r.Code -eq 99) "exit $($r.Code)"
    $r = Pco --lang ro status
    Check 'C' 'next start undoes it and says so' ($r.Code -eq 0 -and $r.Out.Contains('Am anulat-o automat: 1 modificări restaurate.'))
    $s = Snap 'c-after' 'c-before'
    Check 'C' 'registry exactly as before the interrupted apply' ($s.Code -eq 0)

    if ($orig) { Set-ItemProperty -Path $gameBar -Name AutoGameModeEnabled -Value $orig.AutoGameModeEnabled -Type DWord }
    else { Remove-ItemProperty -Path $gameBar -Name AutoGameModeEnabled -ErrorAction SilentlyContinue }
    if (-not $keyExisted) { Remove-Item -Path $gameBar -ErrorAction SilentlyContinue }

    # --- D1: a "game" is running ------------------------------------------------------------
    $fakeGame = Join-Path $Work 'cs2.exe'
    Copy-Item "$env:WINDIR\System32\timeout.exe" $fakeGame
    $proc = Start-Process -FilePath $fakeGame -ArgumentList '600' -PassThru -WindowStyle Minimized
    Start-Sleep -Seconds 2
    $s = Snap 'd-before'
    $r = Pco --lang ro apply gaming.game-mode-on
    Check 'D1' 'game running → blocked (exit 2)' ($r.Code -eq 2 -and $r.Out.Contains('Închide jocul întâi: Counter-Strike 2 (cs2.exe)')) "exit $($r.Code)"
    $s = Snap 'd-after' 'd-before'
    Check 'D1' 'nothing changed while the game ran' ($s.Code -eq 0)
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
}

# --- Report ----------------------------------------------------------------------------------
$failed = @($report | Where-Object Result -eq 'FAIL').Count
$md = @("# VM test report", '', "Windows build $build, administrator: $elevated", '',
        '| Test | Check | Result | Detail |', '| --- | --- | --- | --- |')
foreach ($row in $report) {
    $md += "| $($row.Test) | $($row.Check) | $($row.Result) | $(($row.Detail -replace '\|', '/' -replace "`r?`n", ' ').Trim()) |"
}
$md += ''
$md += if ($failed -eq 0) { 'ALL CHECKS PASSED' } else { "$failed CHECK(S) FAILED" }
$md | Set-Content -Encoding UTF8 -Path (Join-Path $Work 'report.md')

Write-Host ''
$report | Format-Table -AutoSize | Out-String -Width 200 | Write-Host
Write-Host "Report: $(Join-Path $Work 'report.md')   Full log: $log"
if ($failed -eq 0) {
    Write-Host 'ALL CHECKS PASSED' -ForegroundColor Green
    exit 0
}
Write-Host "$failed CHECK(S) FAILED" -ForegroundColor Red
exit 1
