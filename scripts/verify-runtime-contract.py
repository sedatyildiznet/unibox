#!/usr/bin/env python3
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
CORE = (ROOT / 'crates/unibox-core/src/lib.rs').read_text(encoding='utf-8')
BOOTSTRAP = (ROOT / 'apps/desktop/src-tauri/resources/runtime/bootstrap.ps1').read_text(encoding='utf-8')
PROVISION = (ROOT / 'apps/desktop/src-tauri/resources/runtime/provision-runtime.sh').read_text(encoding='utf-8')
CONNECTOR = (ROOT / 'apps/desktop/src-tauri/resources/runtime/unibox-connector.sh').read_text(encoding='utf-8')
TAURI = (ROOT / 'apps/desktop/src-tauri/tauri.conf.json').read_text(encoding='utf-8')

checks = {
    'core uses managed Synapse service name': 'pub const SYNAPSE_SERVICE: &str = "unibox-synapse";' in CORE,
    'core does not start distro package service': 'systemctl start postgresql matrix-synapse' not in CORE,
    'bootstrap imports WSL2 runtime': "'--version', '2'" in BOOTSTRAP,
    'bootstrap verifies SHA-256': 'Get-FileHash -Algorithm SHA256' in BOOTSTRAP,
    'bootstrap starts managed Synapse service': 'postgresql unibox-synapse' in BOOTSTRAP,
    'bootstrap can request WSL installation': "--install', '--no-distribution'" in BOOTSTRAP,
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
