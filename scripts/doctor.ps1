$ErrorActionPreference = 'Continue'

$Distro = 'UniboxRuntime'
$DataRoot = Join-Path $env:LOCALAPPDATA 'Unibox'
$checks = [System.Collections.Generic.List[object]]::new()

function Add-Check([string]$Name, [bool]$OK, [string]$Details, [bool]$Critical = $true) {
    $checks.Add([PSCustomObject]@{
        Name = $Name
        OK = $OK
        Critical = $Critical
        Details = $Details
    })
}

function Invoke-WslCapture([string]$Command) {
    $output = & wsl.exe -d $Distro -- bash -lc $Command 2>&1
    if ($LASTEXITCODE -ne 0) { throw ($output -join "`n") }
    return (($output -join "`n") -replace "`0", '').Trim()
}

$wslCommand = Get-Command wsl.exe -ErrorAction SilentlyContinue
Add-Check 'WSL command' ([bool]$wslCommand) $(if ($wslCommand) { $wslCommand.Source } else { 'wsl.exe not found' })

$wslReady = $false
if ($wslCommand) {
    & wsl.exe --status *> $null
    $wslReady = $LASTEXITCODE -eq 0
}
Add-Check 'WSL2 ready' $wslReady $(if ($wslReady) { 'WSL reports a healthy status' } else { 'WSL is not ready; Windows restart or virtualization setup may be required' })

$distroInstalled = $false
$distroVersion2 = $false
if ($wslReady) {
    $list = ((& wsl.exe -l -q 2>$null) -join "`n") -replace "`0", ''
    $distroInstalled = ($list -split "`r?`n" | ForEach-Object { $_.Trim() }) -contains $Distro
    if ($distroInstalled) {
        $verbose = ((& wsl.exe -l -v 2>$null) -join "`n") -replace "`0", ''
        $line = $verbose -split "`r?`n" | Where-Object { $_ -match [regex]::Escape($Distro) } | Select-Object -First 1
        $distroVersion2 = [bool]($line -match '\s2\s*$')
    }
}
Add-Check 'UniboxRuntime installed' $distroInstalled $(if ($distroInstalled) { 'Managed local runtime is registered' } else { 'Managed local runtime is not registered' })
Add-Check 'UniboxRuntime uses WSL2' $distroVersion2 $(if ($distroVersion2) { 'WSL version 2' } else { 'Runtime is missing or not using WSL2' })

Add-Check 'Local data directory' (Test-Path $DataRoot) $DataRoot $false

if ($distroInstalled) {
    try {
        $postgres = Invoke-WslCapture 'systemctl is-active postgresql'
        Add-Check 'PostgreSQL' ($postgres -eq 'active') $postgres
    } catch {
        Add-Check 'PostgreSQL' $false $_.Exception.Message
    }

    try {
        $synapse = Invoke-WslCapture 'systemctl is-active unibox-synapse'
        Add-Check 'Synapse' ($synapse -eq 'active') $synapse
    } catch {
        Add-Check 'Synapse' $false $_.Exception.Message
    }

    try {
        $session = Invoke-WslCapture 'test -s /var/lib/unibox/matrix.json && printf ready'
        Add-Check 'Local Matrix session' ($session -eq 'ready') 'Local session file exists; credentials are intentionally not displayed'
    } catch {
        Add-Check 'Local Matrix session' $false 'Local Matrix session is missing or unreadable'
    }

    try {
        $versionJson = Invoke-WslCapture 'cat /var/lib/unibox/runtime-version.json 2>/dev/null || true'
        if ($versionJson) {
            $version = $versionJson | ConvertFrom-Json
            Add-Check 'Runtime versions' $true "Synapse $($version.synapse); PostgreSQL $($version.postgres); $($version.os)" $false
        } else {
            Add-Check 'Runtime versions' $false 'runtime-version.json not found' $false
        }
    } catch {
        Add-Check 'Runtime versions' $false 'Could not read runtime version metadata' $false
    }

    try {
        $connectors = Invoke-WslCapture "systemctl list-unit-files 'unibox-*.service' --no-legend --no-pager | awk '{print `$1}' | grep -v '^unibox-synapse.service$' || true"
        if ($connectors) {
            foreach ($service in ($connectors -split "`r?`n" | Where-Object { $_ })) {
                try {
                    $state = Invoke-WslCapture "systemctl is-active '$service' || true"
                    Add-Check "Connector: $service" ($state -eq 'active') $state $false
                } catch {
                    Add-Check "Connector: $service" $false 'Unable to query connector state' $false
                }
            }
        } else {
            Add-Check 'Connectors' $true 'No connectors installed yet' $false
        }
    } catch {
        Add-Check 'Connectors' $false 'Unable to enumerate connectors' $false
    }
}

try {
    $response = Invoke-WebRequest -UseBasicParsing -Uri 'http://127.0.0.1:8008/_matrix/client/versions' -TimeoutSec 3
    Add-Check 'Matrix client API' ($response.StatusCode -eq 200) "HTTP $($response.StatusCode) on localhost only"
} catch {
    Add-Check 'Matrix client API' $false 'Local Matrix client endpoint is not responding'
}

Write-Host ''
Write-Host 'Unibox Doctor' -ForegroundColor Cyan
Write-Host '-------------'
$checks | Format-Table -AutoSize Name, OK, Critical, Details

$criticalFailures = @($checks | Where-Object { $_.Critical -and -not $_.OK })
if ($criticalFailures.Count -gt 0) {
    Write-Host "`n$($criticalFailures.Count) critical check(s) failed." -ForegroundColor Red
    exit 1
}

Write-Host "`nAll critical checks passed." -ForegroundColor Green
exit 0
