<#
.SYNOPSIS
  Builds the Windows installer (Setup.exe, NSIS) with the app, the elevated helper, the
  `pco` command line (used by the uninstaller to undo everything) and the pinned PresentMon.

  Output: app\src-tauri\target\release\bundle\nsis\*-setup.exe

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\build-installer.ps1
#>
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$triple = 'x86_64-pc-windows-msvc'

function Invoke-Checked([string]$what, [scriptblock]$block) {
    & $block
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit $LASTEXITCODE)" }
}

Push-Location $root
try {
    # PresentMon, checked against its pinned SHA-256.
    Invoke-Checked 'fetch PresentMon' { powershell -ExecutionPolicy Bypass -File scripts\fetch-presentmon.ps1 }

    # The helper and pco, as sidecars named the way Tauri expects (<name>-<target triple>.exe).
    Invoke-Checked 'cargo build' { cargo build --release -p pco-helper -p pco-cli }
    $bin = Join-Path $root 'app\src-tauri\binaries'
    New-Item -ItemType Directory -Force -Path $bin | Out-Null
    Copy-Item (Join-Path $root 'target\release\pco-helper.exe') (Join-Path $bin "pco-helper-$triple.exe") -Force
    Copy-Item (Join-Path $root 'target\release\pco.exe') (Join-Path $bin "pco-$triple.exe") -Force

    Push-Location (Join-Path $root 'app')
    try {
        Invoke-Checked 'npm ci' { npm ci }
        Invoke-Checked 'tauri build' { npx tauri build --config src-tauri/tauri.installer.json }
    } finally {
        Pop-Location
    }

    $setup = Get-ChildItem (Join-Path $root 'app\src-tauri\target\release\bundle\nsis') -Filter '*-setup.exe' |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $setup) { throw 'no installer was built' }
    Write-Host "Installer: $($setup.FullName)"
    $setup.FullName
} finally {
    Pop-Location
}
