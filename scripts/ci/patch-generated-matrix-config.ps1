param(
    [Parameter(Mandatory = $true)]
    [string]$Path,
    [string]$Domain = 'unibox.local',
    [string]$Address = 'http://127.0.0.1:8008'
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    throw "Generated connector config does not exist: $Path"
}

$lines = [System.Collections.Generic.List[string]]::new()
Get-Content -LiteralPath $Path | ForEach-Object { [void]$lines.Add($_) }

$section = ''
$domainSet = $false
$addressSet = $false

for ($i = 0; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]

    if ($line -match '^([A-Za-z0-9_-]+):\s*(?:#.*)?$') {
        $section = $Matches[1]
        continue
    }

    if ($section -ne 'homeserver') {
        continue
    }

    if ($line -match '^(\s*)domain:\s*.*$') {
        $lines[$i] = "$($Matches[1])domain: $Domain"
        $domainSet = $true
        continue
    }

    if ($line -match '^(\s*)address:\s*.*$') {
        $lines[$i] = "$($Matches[1])address: $Address"
        $addressSet = $true
        continue
    }
}

if (-not $domainSet) {
    throw 'Generated connector config has no homeserver.domain key; upstream config layout changed.'
}
if (-not $addressSet) {
    throw 'Generated connector config has no homeserver.address key; upstream config layout changed.'
}

Set-Content -LiteralPath $Path -Value $lines -Encoding utf8
Write-Host "Patched generated Matrix homeserver config in $Path"
