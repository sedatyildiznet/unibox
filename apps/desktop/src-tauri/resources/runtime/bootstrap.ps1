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

function Request-WslInstall {
    Write-Output 'WSL2 is not ready. Requesting Windows elevation to enable it...'
    $process = Start-Process -FilePath 'wsl.exe' -ArgumentList @('--install', '--no-distribution') -Verb RunAs -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "Windows could not enable WSL2 automatically (exit code $($process.ExitCode))."
    }
    throw 'WSL2 was enabled. Restart Windows once, then reopen Unibox to finish installing the local engine.'
}

function Ensure-Wsl {
    if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
        throw 'This Windows installation does not provide wsl.exe. Unibox requires a supported 64-bit Windows 10/11 installation with WSL2.'
    }

    & wsl.exe --status *> $null
    if ($LASTEXITCODE -ne 0) {
        Request-WslInstall
    }

    & wsl.exe --set-default-version 2 *> $null
    if ($LASTEXITCODE -ne 0) {
        throw 'WSL is installed, but WSL2 could not be selected as the default runtime. Ensure virtualization is enabled and restart Windows.'
    }
}

function Test-DistroExists {
    $distros = (& wsl.exe -l -q 2>$null) -replace "`0", ''
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
    & wsl.exe --import $Distro $DistroDir $RootfsPath --version 2
    if ($LASTEXITCODE -ne 0) {
        throw 'Failed to import the UniboxRuntime WSL distribution.'
    }

    & wsl.exe -d $Distro -- bash -lc "printf '[boot]\nsystemd=true\n' > /etc/wsl.conf"
    if ($LASTEXITCODE -ne 0) {
        & wsl.exe --unregister $Distro *> $null
        throw 'Failed to configure systemd in UniboxRuntime.'
    }
    & wsl.exe --terminate $Distro | Out-Null
    Start-Sleep -Milliseconds 800
}

function Invoke-LinuxScript([string]$Path) {
    $content = Get-Content -Raw -Encoding UTF8 $Path
    $content | & wsl.exe -d $Distro -- bash -s
    if ($LASTEXITCODE -ne 0) {
        throw "Runtime provisioning script failed: $Path"
    }
}

Ensure-Wsl
New-Item -ItemType Directory -Force -Path $DataRoot | Out-Null

if (-not (Test-DistroExists)) {
    Download-Rootfs
    Import-Runtime
}

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

Write-Output 'UniboxRuntime is installed and healthy.'
