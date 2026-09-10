param(
    [string]$DataRoot = (Join-Path $env:LOCALAPPDATA 'app.unibox.desktop\runtime'),
    [switch]$Prepare,
    [string]$ReportPath = (Join-Path $PWD 'runtime-e2e.json')
)
# Run only on a dedicated Windows 11 machine with virtualization support.
$ErrorActionPreference = 'Stop'
$checks = [ordered]@{}
function Check([string]$Name, [scriptblock]$Test) {
    try { & $Test; $checks[$Name] = 'passed' }
    catch { $checks[$Name] = 'failed' }
}
function Wsl([string]$Shell) {
    $oldPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = & wsl.exe -d UniboxRuntime -- bash -lc $Shell 2>$null
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $oldPreference }
    if ($code -ne 0) { throw 'Runtime check failed.' }
    return $output
}
if ($Prepare) {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot '../apps/desktop/src-tauri/resources/runtime/bootstrap.ps1') -DataRoot $DataRoot
    $state = Get-Content -LiteralPath (Join-Path $DataRoot 'bootstrap-state.json') -Raw | ConvertFrom-Json
    if ($state.state -eq 'REBOOT_REQUIRED') {
        Write-Host 'Restart Windows, then rerun this test with -Prepare.'
        exit 3010
    }
    if ($state.state -ne 'RUNTIME_READY') { throw 'Local engine setup did not finish.' }
}
Check 'systemd' { $null = Wsl 'systemctl list-units --no-pager >/dev/null' }
Check 'database' { $null = Wsl 'runuser -u postgres -- psql -d synapse -tAc "SELECT 1" >/dev/null' }
Check 'homeserver' {
    $null = Invoke-RestMethod 'http://127.0.0.1:8008/_matrix/client/versions' -TimeoutSec 10
}
Check 'local_session' {
    $session = (Wsl 'cat /var/lib/unibox/matrix.json') | ConvertFrom-Json
    if ($session.homeserver -ne 'http://127.0.0.1:8008') { throw 'Unexpected endpoint.' }
    $who = Invoke-RestMethod 'http://127.0.0.1:8008/_matrix/client/v3/account/whoami' -Headers @{ Authorization = "Bearer $($session.access_token)" } -TimeoutSec 10
    if ($who.user_id -ne $session.user_id) { throw 'Session mismatch.' }
}
Check 'client_only_listener' {
    $null = Wsl '/opt/unibox/synapse-venv/bin/python -c "import yaml; c=yaml.safe_load(open(\"/etc/unibox/homeserver.yaml\")); assert c[\"enable_registration\"] is False; assert \"registration_shared_secret\" not in c; assert all(set(x[\"bind_addresses\"]) <= {\"127.0.0.1\",\"::1\"} for x in c[\"listeners\"]); assert all(\"federation\" not in r[\"names\"] for x in c[\"listeners\"] for r in x[\"resources\"])"'
}
Check 'connector_manager' { $null = Wsl 'test -x /opt/unibox/bin/unibox-connector' }
# Only test names and outcomes are exported. No raw output or credentials.
[ordered]@{ schema = 1; checks = $checks; messaging_connectors = 'not_tested'; signed_update = 'not_tested' } |
    ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $ReportPath -Encoding UTF8
if ($checks.Values -contains 'failed') { Write-Error 'Runtime acceptance checks failed. See the redacted report.'; exit 1 }
Write-Host 'Local runtime checks passed. Provider messaging still requires manual acceptance.'
exit 0
