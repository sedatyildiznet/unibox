param(
    [Parameter(Mandatory = $true)]
    [string]$DataRoot
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$OutputEncoding = [Console]::OutputEncoding
$ProgressPreference = 'SilentlyContinue'
$script:SetupLock = $null

$Distro = 'UniboxRuntime'
$BaseUrl = 'https://cloud-images.ubuntu.com/wsl/releases/noble/current'
$RootfsName = 'ubuntu-noble-wsl-amd64-wsl.rootfs.tar.gz'
$DownloadDir = Join-Path $DataRoot 'downloads'
$DistroDir = Join-Path $DataRoot 'wsl'
$RootfsPath = Join-Path $DownloadDir $RootfsName
$SumPath = Join-Path $DownloadDir 'SHA256SUMS'

function Invoke-WslProbe {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    # Windows PowerShell 5.1 turns native stderr into NativeCommandError records.
    # With the script-wide ErrorActionPreference=Stop that would terminate the
    # bootstrap before we can inspect wsl.exe's real exit code. Probe commands
    # are therefore run with non-terminating native stderr and evaluated solely
    # by their process exit code.
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        & wsl.exe @Arguments *> $null
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }

    return $exitCode
}

function Write-BootstrapState([string]$State, [string]$Message) {
    $result = [ordered]@{ state = $State; message = $Message; boot_id = $script:BootId }
    $json = $result | ConvertTo-Json -Compress
    $temporary = Join-Path $DataRoot 'bootstrap-state.tmp'
    [IO.File]::WriteAllText($temporary, $json, (New-Object System.Text.UTF8Encoding($false)))
    Move-Item -LiteralPath $temporary -Destination (Join-Path $DataRoot 'bootstrap-state.json') -Force
    [Console]::WriteLine("UNIBOX_BOOTSTRAP:$json")
}

function Request-WslInstall {
    Write-BootstrapState 'INSTALLING_WSL' 'Preparing Windows. Please approve the Windows permission request.'
    $process = Start-Process -FilePath 'wsl.exe' -ArgumentList @('--install', '--no-distribution') -Verb RunAs -Wait -PassThru
    if ($process.ExitCode -notin @(0, 3010, 1641)) {
        throw "Windows could not enable WSL2 automatically (exit code $($process.ExitCode))."
    }
    Write-BootstrapState 'REBOOT_REQUIRED' 'Windows needs to restart once. Reopen Unibox afterward to continue automatically.'
    return $false
}

function Ensure-Wsl {
    if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
        throw 'This Windows installation does not provide wsl.exe. Unibox requires a supported 64-bit Windows 10/11 installation with WSL2.'
    }

    if ((Invoke-WslProbe -Arguments @('--status')) -ne 0) {
        return (Request-WslInstall)
    }

    if ((Invoke-WslProbe -Arguments @('--set-default-version', '2')) -ne 0) {
        throw 'WSL is installed, but WSL2 could not be selected as the default runtime. Ensure virtualization is enabled and restart Windows.'
    }
    return $true
}

function Test-DistroExists {
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $distros = (& wsl.exe -l -q 2>$null) -replace "`0", ''
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }

    if ($exitCode -ne 0) {
        return $false
    }

    return ($distros | ForEach-Object { $_.Trim() }) -contains $Distro
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
        $partial = "$RootfsPath.partial"
        Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/$RootfsName" -OutFile $partial
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $partial).Hash.ToLowerInvariant() -ne $expected) {
            Remove-Item -LiteralPath $partial -Force
            throw 'Ubuntu rootfs SHA-256 verification failed.'
        }
        Move-Item -LiteralPath $partial -Destination $RootfsPath -Force
    }

    $actual = (Get-FileHash -Algorithm SHA256 -Path $RootfsPath).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        Remove-Item $RootfsPath -Force -ErrorAction SilentlyContinue
        throw 'Ubuntu rootfs SHA-256 verification failed.'
    }
}

function Import-Runtime {
    New-Item -ItemType Directory -Force -Path $DistroDir | Out-Null
    & wsl.exe --import $Distro $DistroDir $RootfsPath --version 2
    if ($LASTEXITCODE -ne 0) {
        throw 'Failed to import the UniboxRuntime WSL distribution.'
    }

}

function Enable-RuntimeSystemd {
    & wsl.exe -d $Distro -- bash -lc "printf '[boot]\nsystemd=true\n' > /etc/wsl.conf"
    if ($LASTEXITCODE -ne 0) { throw 'Failed to configure local engine startup.' }
    $null = Invoke-WslProbe -Arguments @('--terminate', $Distro)
}

function Invoke-LinuxScript([string]$Path) {
    $content = Get-Content -Raw -Encoding UTF8 $Path
    $content | & wsl.exe -d $Distro -- bash -s
    if ($LASTEXITCODE -ne 0) {
        throw "Runtime provisioning script failed: $Path"
    }
}

try {
    New-Item -ItemType Directory -Force -Path $DataRoot | Out-Null
    $script:SetupLock = [IO.File]::Open((Join-Path $DataRoot 'bootstrap.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
    $script:BootId = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o')
    $statePath = Join-Path $DataRoot 'bootstrap-state.json'
    if (Test-Path -LiteralPath $statePath) {
        try { $previous = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json }
        catch { $previous = $null }
        if ($previous.state -eq 'REBOOT_REQUIRED' -and $previous.boot_id -eq $script:BootId) {
            Write-BootstrapState 'REBOOT_REQUIRED' 'Windows needs to restart once. Reopen Unibox afterward to continue automatically.'
            exit 0
        }
    }
    if (-not (Ensure-Wsl)) { exit 0 }
    Write-BootstrapState 'RUNTIME_INSTALLING' 'Preparing your private local engine. This can take several minutes.'

    if (-not (Test-DistroExists)) {
        $drive = New-Object IO.DriveInfo([IO.Path]::GetPathRoot($DataRoot))
        if ($drive.AvailableFreeSpace -lt 8GB) { throw 'At least 8 GB of free space is required.' }
        Download-Rootfs
        Import-Runtime
    }

    Enable-RuntimeSystemd

    $ProvisionScript = Join-Path $PSScriptRoot 'provision-runtime.sh'
    $ConnectorScript = Join-Path $PSScriptRoot 'unibox-connector.sh'
    if (-not (Test-Path $ProvisionScript)) { throw 'Missing provision-runtime.sh resource.' }
    if (-not (Test-Path $ConnectorScript)) { throw 'Missing unibox-connector.sh resource.' }

    Invoke-LinuxScript $ProvisionScript

    $connectorContent = Get-Content -Raw -Encoding UTF8 $ConnectorScript
    $connectorContent | & wsl.exe -d $Distro -- bash -lc 'cat > /opt/unibox/bin/unibox-connector && chmod 0755 /opt/unibox/bin/unibox-connector'
    if ($LASTEXITCODE -ne 0) {
        throw 'Failed to install Unibox connector manager.'
    }

    & wsl.exe -d $Distro -- bash -lc 'systemctl enable --now postgresql unibox-synapse >/dev/null && systemctl is-active --quiet postgresql unibox-synapse'
    if ($LASTEXITCODE -ne 0) {
        throw 'Unibox local services did not become healthy.'
    }

    Write-BootstrapState 'RUNTIME_READY' 'Your private local engine is ready.'
}
catch {
    # Never return native output, paths, session material or PowerShell stacks to the UI.
    $message = 'Local engine setup could not finish. Check your connection, available disk space and Windows virtualization settings, then retry.'
    if ($script:SetupLock) { Write-BootstrapState 'ERROR' $message }
    else { [Console]::WriteLine('UNIBOX_BOOTSTRAP:{"state":"ERROR","message":"Setup is already running or its data folder is unavailable. Close other Unibox windows and retry."}') }
    exit 1
}
finally {
    if ($script:SetupLock) { $script:SetupLock.Dispose() }
}
