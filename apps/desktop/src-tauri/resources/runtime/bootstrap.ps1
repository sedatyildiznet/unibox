param(
    [Parameter(Mandatory = $true)]
    [string]$DataRoot
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Distro = 'UniboxRuntime'
$BaseUrl = 'https://cloud-images.ubuntu.com/wsl/releases/noble/current'
$RootfsName = 'ubuntu-noble-wsl-amd64-wsl.rootfs.tar.gz'
$DownloadDir = Join-Path $DataRoot 'downloads'
$DistroDir = Join-Path $DataRoot 'wsl'
$RootfsPath = Join-Path $DownloadDir $RootfsName
$SumPath = Join-Path $DownloadDir 'SHA256SUMS'
$RuntimeDiskPath = Join-Path $DistroDir 'ext4.vhdx'

function Invoke-WslNative {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    # Windows PowerShell 5.1 can promote native stderr to terminating errors when
    # ErrorActionPreference is Stop. Capture the native result ourselves so WSL
    # error codes can be diagnosed reliably.
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $lines = & wsl.exe @Arguments 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }

    $text = (($lines | ForEach-Object { $_.ToString() }) -join "`n").Trim()
    return [pscustomobject]@{
        ExitCode = $exitCode
        Text = $text
    }
}

function Convert-ToUnixText {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    # Runtime resources are packaged on Windows. If Git or the packaging step
    # gives a shell script CRLF line endings, bash reads e.g. "pipefail\r" as
    # the option name. Normalize all shell input to LF immediately before it is
    # sent to WSL so the runtime remains independent of checkout line endings.
    $content = Get-Content -Raw -Encoding UTF8 $Path
    return $content.Replace("`r`n", "`n").Replace("`r", "`n")
}

function Test-FirmwareVirtualizationDisabled {
    try {
        $cpu = Get-CimInstance Win32_Processor -ErrorAction Stop | Select-Object -First 1
        return ($null -ne $cpu.VirtualizationFirmwareEnabled -and -not [bool]$cpu.VirtualizationFirmwareEnabled)
    }
    catch {
        return $false
    }
}

function Repair-WslPlatform {
    param([string]$Reason = 'WSL2 platform is not ready')

    Write-Output "$Reason. Requesting Windows administrator permission to repair WSL2..."

    $repairCommand = @'
$ErrorActionPreference = 'Stop'
$restartRequired = $false

foreach ($feature in @('Microsoft-Windows-Subsystem-Linux', 'VirtualMachinePlatform')) {
    $state = Get-WindowsOptionalFeature -Online -FeatureName $feature
    if ($state.State -ne 'Enabled') {
        $result = Enable-WindowsOptionalFeature -Online -FeatureName $feature -All -NoRestart
        if ($result.RestartNeeded) { $restartRequired = $true }
    }
}

& bcdedit.exe /set hypervisorlaunchtype auto | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not enable the Windows hypervisor boot setting.' }

foreach ($serviceName in @('vmcompute', 'hns', 'WslService', 'LxssManager')) {
    $service = Get-Service -Name $serviceName -ErrorAction SilentlyContinue
    if ($service) {
        try { Start-Service -Name $serviceName -ErrorAction Stop } catch { }
    }
}

try { & wsl.exe --update *> $null } catch { }

if ($restartRequired) { exit 3010 }
exit 0
'@

    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($repairCommand))

    try {
        $process = Start-Process -FilePath 'powershell.exe' `
            -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', $encoded) `
            -Verb RunAs -WindowStyle Hidden -Wait -PassThru
    }
    catch {
        throw 'Unibox needs administrator permission to enable the Windows components required by WSL2. Approve the Windows UAC prompt and try again.'
    }

    if ($process.ExitCode -eq 3010) {
        throw 'WSL2 Windows components were enabled successfully. Restart Windows once, then reopen Unibox and press Install local engine again.'
    }

    if ($process.ExitCode -ne 0) {
        throw "Windows could not repair the WSL2 platform automatically (exit code $($process.ExitCode))."
    }

    Start-Sleep -Milliseconds 800
}

function Ensure-Wsl {
    if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
        throw 'This Windows installation does not provide wsl.exe. Unibox requires a supported 64-bit Windows 10/11 installation with WSL2.'
    }

    $status = Invoke-WslNative -Arguments @('--status')
    $vmcompute = Get-Service -Name 'vmcompute' -ErrorAction SilentlyContinue

    # `wsl --status` may return success even when the Host Compute Service cannot
    # create a WSL2 VM. vmcompute being missing/stopped is therefore checked too.
    if ($status.ExitCode -ne 0 -or -not $vmcompute -or $vmcompute.Status -ne 'Running') {
        Repair-WslPlatform -Reason 'WSL2 virtualization services are unavailable'
        $status = Invoke-WslNative -Arguments @('--status')
        $vmcompute = Get-Service -Name 'vmcompute' -ErrorAction SilentlyContinue
    }

    if ($status.ExitCode -ne 0) {
        throw "WSL is installed but not ready: $($status.Text)"
    }

    if (-not $vmcompute -or $vmcompute.Status -ne 'Running') {
        if (Test-FirmwareVirtualizationDisabled) {
            throw 'Hardware virtualization is disabled in BIOS/UEFI. Enable Intel VT-x/VT-d or AMD-V/SVM, restart Windows, then reopen Unibox.'
        }
        throw 'The Windows virtualization service is still unavailable. Restart Windows once, then reopen Unibox. If it still fails, verify that virtualization is enabled in BIOS/UEFI.'
    }

    $defaultVersion = Invoke-WslNative -Arguments @('--set-default-version', '2')
    if ($defaultVersion.ExitCode -ne 0) {
        if (Test-FirmwareVirtualizationDisabled) {
            throw 'WSL2 cannot start because hardware virtualization is disabled in BIOS/UEFI.'
        }
        throw "WSL2 could not be selected as the default runtime: $($defaultVersion.Text)"
    }
}

function Test-DistroExists {
    $result = Invoke-WslNative -Arguments @('-l', '-q')
    if ($result.ExitCode -ne 0) { return $false }

    $distros = ($result.Text -replace "`0", '') -split "`r?`n"
    return ($distros | ForEach-Object { $_.Trim() }) -contains $Distro
}

function Remove-UnregisteredPartialRuntime {
    # Safe recovery only: if WSL does not know this distro, a leftover directory
    # can only be debris from an interrupted/failed import.
    if ((-not (Test-DistroExists)) -and (Test-Path $DistroDir)) {
        Remove-Item $DistroDir -Recurse -Force -ErrorAction Stop
    }
}

function Reset-StaleRuntimeRegistration {
    # A registered distro whose ext4.vhdx is gone cannot contain usable local
    # data. This is a stale registration left by an interrupted import. It is
    # safe to unregister and recreate it from the verified Ubuntu rootfs.
    if (-not (Test-DistroExists)) {
        return $false
    }

    if (Test-Path $RuntimeDiskPath) {
        return $false
    }

    Write-Output 'UniboxRuntime is registered in WSL but its virtual disk is missing. Rebuilding the incomplete runtime...'
    $unregister = Invoke-WslNative -Arguments @('--unregister', $Distro)
    if ($unregister.ExitCode -ne 0 -and (Test-DistroExists)) {
        $details = if ($unregister.Text) { $unregister.Text } else { "wsl.exe exit code $($unregister.ExitCode)" }
        throw "Could not remove the incomplete UniboxRuntime registration: $details"
    }

    if (Test-Path $DistroDir) {
        Remove-Item $DistroDir -Recurse -Force -ErrorAction Stop
    }
    return $true
}

function Ensure-ExistingRuntimeHealthy {
    if (-not (Test-DistroExists)) {
        return $false
    }

    # Check this before starting the distro. It handles the exact case where WSL
    # still lists UniboxRuntime but the ext4.vhdx from a failed import is missing.
    if (Reset-StaleRuntimeRegistration) {
        return $false
    }

    $probe = Invoke-WslNative -Arguments @('-d', $Distro, '--', 'true')
    if ($probe.ExitCode -eq 0) {
        return $true
    }

    if ($probe.Text -match 'HCS_E_SERVICE_NOT_AVAILABLE|0x80370114|required feature is not installed|Gerekli bir özellik') {
        Repair-WslPlatform -Reason 'The existing UniboxRuntime cannot start because the WSL2 virtualization platform is unavailable'
        $probe = Invoke-WslNative -Arguments @('-d', $Distro, '--', 'true')
        if ($probe.ExitCode -eq 0) {
            return $true
        }
    }

    # Some WSL versions report a stale registration only when they try to mount
    # the disk. If the mount error confirms the expected disk is missing, rebuild
    # automatically rather than telling the user to reboot forever.
    if (($probe.Text -match 'ERROR_PATH_NOT_FOUND|MountDisk|system cannot find the path specified|Sistem belirtilen yolu bulam') -and -not (Test-Path $RuntimeDiskPath)) {
        if (Reset-StaleRuntimeRegistration) {
            return $false
        }
    }

    $details = if ($probe.Text) { $probe.Text } else { "wsl.exe exit code $($probe.ExitCode)" }
    throw "The existing UniboxRuntime could not start. Its local data was preserved and was not deleted. Details: $details"
}

function Download-Rootfs {
    New-Item -ItemType Directory -Force -Path $DownloadDir | Out-Null
    Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/SHA256SUMS" -OutFile $SumPath

    $line = Get-Content $SumPath | Where-Object { $_ -match "\s+$([regex]::Escape($RootfsName))$" } | Select-Object -First 1
    if (-not $line) {
        throw "Ubuntu checksum manifest does not contain $RootfsName"
    }
    $expected = ($line -split '\s+')[0].ToLowerInvariant()

    $validExisting = $false
    if (Test-Path $RootfsPath) {
        $actual = (Get-FileHash -Algorithm SHA256 -Path $RootfsPath).Hash.ToLowerInvariant()
        $validExisting = $actual -eq $expected
    }

    if (-not $validExisting) {
        Remove-Item $RootfsPath -Force -ErrorAction SilentlyContinue
        Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/$RootfsName" -OutFile $RootfsPath
    }

    $actual = (Get-FileHash -Algorithm SHA256 -Path $RootfsPath).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        Remove-Item $RootfsPath -Force -ErrorAction SilentlyContinue
        throw 'Ubuntu rootfs SHA-256 verification failed.'
    }
}

function Import-Runtime {
    New-Item -ItemType Directory -Force -Path $DistroDir | Out-Null

    $import = Invoke-WslNative -Arguments @('--import', $Distro, $DistroDir, $RootfsPath, '--version', '2')
    if ($import.ExitCode -ne 0 -and ($import.Text -match 'HCS_E_SERVICE_NOT_AVAILABLE|0x80370114|required feature is not installed|Gerekli bir özellik')) {
        Repair-WslPlatform -Reason 'Windows Host Compute Service cannot create the WSL2 virtual machine'
        Remove-UnregisteredPartialRuntime
        New-Item -ItemType Directory -Force -Path $DistroDir | Out-Null
        $import = Invoke-WslNative -Arguments @('--import', $Distro, $DistroDir, $RootfsPath, '--version', '2')
    }

    if ($import.ExitCode -ne 0) {
        if (Test-FirmwareVirtualizationDisabled) {
            throw 'UniboxRuntime could not be created because hardware virtualization is disabled in BIOS/UEFI.'
        }
        if (-not (Test-DistroExists)) {
            Remove-UnregisteredPartialRuntime
        }
        $details = if ($import.Text) { $import.Text } else { "wsl.exe exit code $($import.ExitCode)" }
        throw "Failed to import the UniboxRuntime WSL2 distribution: $details"
    }

    $configure = Invoke-WslNative -Arguments @('-d', $Distro, '--', 'bash', '-lc', "printf '[boot]\nsystemd=true\n' > /etc/wsl.conf")
    if ($configure.ExitCode -ne 0) {
        [void](Invoke-WslNative -Arguments @('--unregister', $Distro))
        Remove-UnregisteredPartialRuntime
        throw "Failed to configure systemd in UniboxRuntime: $($configure.Text)"
    }

    [void](Invoke-WslNative -Arguments @('--terminate', $Distro))
    Start-Sleep -Milliseconds 800
}

function Invoke-LinuxScript([string]$Path) {
    $content = Convert-ToUnixText -Path $Path
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = $content | & wsl.exe -d $Distro -- bash -s 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }

    if ($exitCode -ne 0) {
        $details = (($output | ForEach-Object { $_.ToString() }) -join "`n").Trim()
        throw "Runtime provisioning script failed: $details"
    }
}

function Invoke-Main {
    Ensure-Wsl
    New-Item -ItemType Directory -Force -Path $DataRoot | Out-Null

    $runtimeReady = $false
    if (Test-DistroExists) {
        $runtimeReady = Ensure-ExistingRuntimeHealthy
    }

    if (-not $runtimeReady) {
        Remove-UnregisteredPartialRuntime
        Download-Rootfs
        Import-Runtime
    }

    $ProvisionScript = Join-Path $PSScriptRoot 'provision-runtime.sh'
    $ConnectorScript = Join-Path $PSScriptRoot 'unibox-connector.sh'
    if (-not (Test-Path $ProvisionScript)) { throw 'Missing provision-runtime.sh resource.' }
    if (-not (Test-Path $ConnectorScript)) { throw 'Missing unibox-connector.sh resource.' }

    Invoke-LinuxScript $ProvisionScript

    $connectorContent = Convert-ToUnixText -Path $ConnectorScript
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $connectorOutput = $connectorContent | & wsl.exe -d $Distro -- bash -lc 'cat > /opt/unibox/bin/unibox-connector && chmod 0755 /opt/unibox/bin/unibox-connector' 2>&1
        $connectorExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($connectorExitCode -ne 0) {
        $details = (($connectorOutput | ForEach-Object { $_.ToString() }) -join "`n").Trim()
        throw "Failed to install Unibox connector manager: $details"
    }

    $services = Invoke-WslNative -Arguments @('-d', $Distro, '--', 'bash', '-lc', 'systemctl enable --now postgresql unibox-synapse >/dev/null && systemctl is-active --quiet postgresql unibox-synapse')
    if ($services.ExitCode -ne 0) {
        throw "Unibox local services did not become healthy: $($services.Text)"
    }

    Write-Output 'UniboxRuntime is installed and healthy.'
}

try {
    Invoke-Main
}
catch {
    # Emit only the actionable message. The desktop UI should not expose a full
    # PowerShell stack trace to end users.
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
