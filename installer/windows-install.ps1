$ErrorActionPreference = 'Stop'
Write-Host 'Unibox managed runtime bootstrap (development scaffold)'
$wsl = Get-Command wsl.exe -ErrorAction SilentlyContinue
if (-not $wsl) { throw 'WSL2 is required. The production installer will offer to enable required Windows components automatically.' }
wsl.exe --status | Out-Host
Write-Host 'WSL2 is available. Runtime image import and signed package verification are the next implementation milestone.'
