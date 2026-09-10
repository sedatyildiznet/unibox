#!/usr/bin/env python3
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
CORE = (ROOT / 'crates/unibox-core/src/lib.rs').read_text(encoding='utf-8')
BOOTSTRAP = (ROOT / 'apps/desktop/src-tauri/resources/runtime/bootstrap.ps1').read_text(encoding='utf-8')
PROVISION = (ROOT / 'apps/desktop/src-tauri/resources/runtime/provision-runtime.sh').read_text(encoding='utf-8')
CONNECTOR = (ROOT / 'apps/desktop/src-tauri/resources/runtime/unibox-connector.sh').read_text(encoding='utf-8')
TAURI = (ROOT / 'apps/desktop/src-tauri/tauri.conf.json').read_text(encoding='utf-8')
MAIN = (ROOT / 'apps/desktop/src-tauri/src/main.rs').read_text(encoding='utf-8')

imports_wsl2 = (
    '--import' in BOOTSTRAP
    and '$Distro' in BOOTSTRAP
    and '$DistroDir' in BOOTSTRAP
    and '$RootfsPath' in BOOTSTRAP
    and '--version' in BOOTSTRAP
    and "'2'" in BOOTSTRAP
)

repairs_wsl2 = (
    'Enable-WindowsOptionalFeature' in BOOTSTRAP
    and 'Microsoft-Windows-Subsystem-Linux' in BOOTSTRAP
    and 'VirtualMachinePlatform' in BOOTSTRAP
    and 'hypervisorlaunchtype' in BOOTSTRAP
    and 'vmcompute' in BOOTSTRAP
)

handles_hcs_failure = (
    'HCS_E_SERVICE_NOT_AVAILABLE' in BOOTSTRAP
    and 'Repair-WslPlatform' in BOOTSTRAP
)

recovers_stale_runtime = (
    'Reset-StaleRuntimeRegistration' in BOOTSTRAP
    and 'ext4.vhdx' in BOOTSTRAP
    and 'ERROR_PATH_NOT_FOUND' in BOOTSTRAP
    and "@('--unregister', $Distro)" in BOOTSTRAP
)

checks = {
    'core uses managed Synapse service name': 'pub const SYNAPSE_SERVICE: &str = "unibox-synapse";' in CORE,
    'core does not start distro package service': 'systemctl start postgresql matrix-synapse' not in CORE,
    'Windows release binary uses GUI subsystem': 'windows_subsystem = "windows"' in MAIN,
    'bootstrap imports WSL2 runtime': imports_wsl2,
    'bootstrap verifies SHA-256': 'Get-FileHash -Algorithm SHA256' in BOOTSTRAP,
    'bootstrap starts managed Synapse service': 'postgresql unibox-synapse' in BOOTSTRAP,
    'bootstrap can repair required WSL2 Windows features': repairs_wsl2,
    'bootstrap handles HCS service-not-available failures': handles_hcs_failure,
    'bootstrap rebuilds stale registered runtime with missing VHDX': recovers_stale_runtime,
    'provision pins Synapse version': 'SYNAPSE_VERSION=1.160.0' in PROVISION,
    'Synapse client listener is loopback only': "bind_addresses: ['127.0.0.1']" in PROVISION,
    'public registration stays disabled': 'enable_registration: false' in PROVISION,
    'local Matrix account is non-admin': '--no-admin' in PROVISION,
    'Synapse systemd service is sandboxed': 'ProtectSystem=strict' in PROVISION and 'NoNewPrivileges=true' in PROVISION,
    'connector services depend on managed Synapse': 'Requires=unibox-synapse.service postgresql.service' in CONNECTOR,
    'connector services are sandboxed': 'ProtectSystem=strict' in CONNECTOR and 'NoNewPrivileges=true' in CONNECTOR,
    'bad appservice registration is rolled back': 'unibox-appservice-unregister "$target"' in CONNECTOR and 'rolled back safely' in CONNECTOR,
    'binary connector update supports rollback': 'update failed and was rolled back' in CONNECTOR,
    'source connector update supports rollback': 'source update failed and was rolled back' in CONNECTOR,
    'Google Chat update uses atomic venv rollback': 'update_python_googlechat()' in CONNECTOR and 'Python connector update failed and was rolled back' in CONNECTOR,
    'desktop updater uses GitHub Releases': 'https://github.com/sedatyildiznet/unibox/releases/latest/download/latest.json' in TAURI,
}

failed = [name for name, ok in checks.items() if not ok]
for name, ok in checks.items():
    print(f"{'OK' if ok else 'FAIL'}: {name}")

if failed:
    print('\nRuntime contract verification failed:', file=sys.stderr)
    for item in failed:
        print(f'- {item}', file=sys.stderr)
    raise SystemExit(1)

print(f'Validated {len(checks)} managed-runtime invariants.')
