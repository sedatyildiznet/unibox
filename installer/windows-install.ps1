$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if (-not (Test-IsAdministrator)) {
    $process = Start-Process -FilePath 'powershell.exe' `
        -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"") `
        -Verb RunAs -Wait -PassThru
    exit $process.ExitCode
}

$restartRequired = $false
foreach ($feature in @('Microsoft-Windows-Subsystem-Linux', 'VirtualMachinePlatform')) {
    $state = Get-WindowsOptionalFeature -Online -FeatureName $feature
    if ($state.State -ne 'Enabled') {
        Write-Host "Enabling $feature..."
        $result = Enable-WindowsOptionalFeature -Online -FeatureName $feature -All -NoRestart
        if ($result.RestartNeeded) { $restartRequired = $true }
    }
}

& bcdedit.exe /set hypervisorlaunchtype auto | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not enable the Windows hypervisor boot setting.' }

foreach ($serviceName in @('vmcompute', 'hns', 'WslService', 'LxssManager')) {
    if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) {
        try { Start-Service -Name $serviceName -ErrorAction Stop } catch { }
    }
}

if (Get-Command wsl.exe -ErrorAction SilentlyContinue) {
    try { wsl.exe --update | Out-Null } catch { }
}

if ($restartRequired) {
    Write-Host 'WSL2 components were enabled. Restart Windows, then launch Unibox again.'
    exit 3010
}

if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
    throw 'wsl.exe is unavailable after enabling Windows components. Restart Windows and try again.'
}

wsl.exe --set-default-version 2 | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw 'WSL2 is still unavailable. Verify virtualization (Intel VT-x/AMD-V/SVM) is enabled in BIOS/UEFI.'
}

Write-Host 'WSL2 platform is ready for Unibox.'
