<#
.SYNOPSIS
  Saves the registry keys the Stage 0 recipes touch, and optionally compares them with an
  earlier save. Read-only: it never changes the registry.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File C:\pco\vm-snapshot.ps1 -Out C:\pco\inainte
  powershell -ExecutionPolicy Bypass -File C:\pco\vm-snapshot.ps1 -Out C:\pco\dupa -CompareWith C:\pco\inainte
#>
param(
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$CompareWith
)

# Keep in sync with recipes/ (one entry per key a recipe writes to).
$keys = @(
    'HKCU\Software\Microsoft\GameBar',
    'HKCU\System\GameConfigStore',
    'HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR',
    'HKCU\Control Panel\Accessibility\StickyKeys',
    'HKCU\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo',
    'HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\AdvertisingInfo',
    'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings'
)

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$i = 0
foreach ($key in $keys) {
    $i++
    $file = Join-Path $Out ('{0:D2}.txt' -f $i)
    # `reg query /s` lists every value (and subkey) under the key, or fails if it is absent.
    $lines = & reg.exe query $key /s 2>$null
    $state = if ($LASTEXITCODE -eq 0) { "PRESENT: $key" } else { "ABSENT: $key" }
    # The first line always exists, so even an empty key produces a file.
    $content = @($state) + @($lines | Where-Object { $_.Trim() -ne '' })
    Set-Content -Encoding UTF8 -Path $file -Value $content
}
Write-Host "Saved $($keys.Count) keys to $Out"

if ($CompareWith) {
    $different = 0
    for ($n = 1; $n -le $keys.Count; $n++) {
        $name = '{0:D2}.txt' -f $n
        $fileA = Join-Path $CompareWith $name
        $fileB = Join-Path $Out $name
        if (-not (Test-Path $fileA) -or -not (Test-Path $fileB)) {
            Write-Host "ERROR: missing $name in $CompareWith or $Out; cannot compare." -ForegroundColor Red
            exit 2
        }
        $a = Get-Content $fileA
        $b = Get-Content $fileB
        $diff = Compare-Object $a $b
        if ($diff) {
            $different++
            Write-Host ''
            Write-Host "DIFFERENT: $($keys[$n - 1])" -ForegroundColor Red
            $diff | ForEach-Object {
                $side = if ($_.SideIndicator -eq '<=') { 'before' } else { 'after ' }
                Write-Host "  $side  $($_.InputObject)"
            }
        }
    }
    Write-Host ''
    if ($different -eq 0) {
        Write-Host 'IDENTICAL: every key is exactly as before.' -ForegroundColor Green
    } else {
        Write-Host "$different key(s) differ (see above)." -ForegroundColor Red
        exit 1
    }
}
