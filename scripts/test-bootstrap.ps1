# Behavioral regression tests. This script never enables WSL or restarts Windows.
$ErrorActionPreference = 'Stop'
$path = Join-Path $PSScriptRoot '../apps/desktop/src-tauri/resources/runtime/bootstrap.ps1'
$tokens = $null
$errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path $path), [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'Bootstrap PowerShell syntax is invalid.' }
$functions = $ast.FindAll({ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $false)
foreach ($definition in $functions) { Invoke-Expression $definition.Extent.Text }
$DataRoot = Join-Path ([IO.Path]::GetTempPath()) ('Unibox test ' + [guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $DataRoot | Out-Null
$script:BootId = 'test-boot'
try {
    function Start-Process { return [pscustomobject]@{ ExitCode = $script:InstallExitCode } }
    foreach ($code in @(0, 3010, 1641)) {
        $script:InstallExitCode = $code
        $result = Request-WslInstall
        if ($result -ne $false) { throw 'Expected reboot-required to stop provisioning without an exception.' }
        $state = Get-Content (Join-Path $DataRoot 'bootstrap-state.json') -Raw | ConvertFrom-Json
        if ($state.state -ne 'REBOOT_REQUIRED' -or $state.boot_id -ne 'test-boot') { throw 'Expected durable reboot state.' }
    }
    $script:InstallExitCode = 5
    $failed = $false
    try { $null = Request-WslInstall } catch { $failed = $true }
    if (-not $failed) { throw 'A failed WSL installation must not become success.' }
    function wsl.exe { Write-Error 'Native probe stderr'; $global:LASTEXITCODE = 17 }
    if ((Invoke-WslProbe -Arguments @('--status')) -ne 17) { throw 'Native probe exit code was lost.' }
    if ($ErrorActionPreference -ne 'Stop') { throw 'Probe failed to restore error preference.' }
    function wsl.exe { Write-Error 'Native informational stderr'; $global:LASTEXITCODE = 0 }
    Invoke-WslCommand -Arguments @('--status')
    function wsl.exe { Write-Error 'Native failure stderr'; $global:LASTEXITCODE = 3 }
    $failed = $false
    try { Invoke-WslCommand -Arguments @('--status') } catch { $failed = $true }
    if (-not $failed) { throw 'A failed native command must not become success.' }
    Write-BootstrapState 'RUNTIME_READY' 'Ready'
    $state = Get-Content (Join-Path $DataRoot 'bootstrap-state.json') -Raw | ConvertFrom-Json
    if ($state.state -ne 'RUNTIME_READY') { throw 'Ready state did not replace reboot state.' }
    Write-Host 'Bootstrap behavioral regression tests passed.'
} finally {
    Remove-Item -LiteralPath $DataRoot -Recurse -Force
}
# The mock intentionally set LASTEXITCODE to 17. Do not leak it into the CI shell.
exit 0
