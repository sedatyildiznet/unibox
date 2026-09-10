#!/opt/unibox/synapse-venv/bin/python
"""Version-matched local data backup/restore. Never archive live PostgreSQL files."""
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

RECOVERY_ROOT = Path('/var/lib/unibox-maintenance')
MAX_FILES = 200000
MAX_BYTES = 100 * 1024**3


def run(*args, output=None, source=None):
    result = subprocess.run(args, stdin=source, stdout=output or subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    if result.returncode:
        raise RuntimeError('A local maintenance command failed.')
    return result.stdout.decode().strip() if output is None else ''


def digest(path):
    with open(path, 'rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def roots():
    result = {'config': Path('/etc/unibox'), 'data': Path('/var/lib/unibox')}
    for connector in Path('/opt/unibox/connectors').iterdir():
        if re.fullmatch(r'[a-z0-9_-]+', connector.name) and (connector / 'state').is_dir():
            result[f'connectors/{connector.name}/state'] = connector / 'state'
    for path in result.values():
        if path.resolve() != path:
            raise RuntimeError('Unexpected symbolic link in local data paths.')
    return result


def fingerprint():
    versions = json.loads(Path('/var/lib/unibox/runtime-version.json').read_text())
    connectors = {}
    for path in Path('/opt/unibox/connectors').iterdir():
        if not (path / 'state').is_dir():
            continue
        binaries = {item.name: digest(item) for item in path.glob('mautrix-*') if item.is_file()}
        python = path / 'venv/bin/python'
        if python.exists():
            binaries['python_packages'] = hashlib.sha256(run(str(python), '-m', 'pip', 'freeze').encode()).hexdigest()
        connectors[path.name] = binaries
    return {'synapse': versions['synapse'], 'postgres_major': versions['postgres'].split('.')[0], 'connectors': connectors}


def databases():
    names = run('runuser', '-u', 'postgres', '--', 'psql', '-Atc', 'SELECT datname FROM pg_database').splitlines()
    return sorted(name for name in names if name == 'synapse' or re.fullmatch(r'unibox_[a-z0-9_]+', name))


def services():
    lines = run('systemctl', 'list-units', '--type=service', '--state=running', '--no-legend', '--plain', 'unibox-*').splitlines()
    return [line.split()[0] for line in lines if line and re.fullmatch(r'unibox-[a-z0-9_-]+\.service', line.split()[0])]


def resume(active):
    if active:
        run('systemctl', 'reset-failed', *active)
        run('systemctl', 'start', *active)
    if active:
        run('systemctl', 'is-active', '--quiet', *active)
    run('bash', '-lc', 'for i in $(seq 1 60); do curl -fsS http://127.0.0.1:8008/_matrix/client/versions >/dev/null 2>&1 && exit 0; sleep 1; done; exit 1')


def collect(stage, sources, dbs):
    for name, source in sources.items():
        for item in source.rglob('*'):
            if item.is_symlink():
                raise RuntimeError('Symbolic links in account data are not supported by backup.')
        shutil.copytree(source, stage / name)
    (stage / 'databases').mkdir()
    for name in dbs:
        with (stage / 'databases' / f'{name}.dump').open('wb') as target:
            run('runuser', '-u', 'postgres', '--', 'pg_dump', '--format=custom', '--no-owner', '--dbname', name, output=target)


def write_archive(stage, destination, versions, dbs, source_names):
    files = {str(item.relative_to(stage)): digest(item) for item in stage.rglob('*') if item.is_file()}
    manifest = {'schema': 1, 'versions': versions, 'databases': dbs, 'roots': sorted(source_names), 'files': files}
    (stage / 'manifest.json').write_text(json.dumps(manifest))
    temporary = destination.with_name(destination.name + '.partial')
    try:
        with tarfile.open(temporary, 'w:gz', dereference=True) as archive:
            for item in sorted(stage.rglob('*')):
                archive.add(item, arcname=str(item.relative_to(stage)), recursive=False)
        os.chmod(temporary, 0o600)
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def unpack_archive(source, destination):
    """Allow only regular files; validate complete names, sizes and hashes before restore."""
    with tarfile.open(source, 'r:gz') as archive:
        names = set()
        file_names = set()
        total = 0
        for member in archive:
            path = PurePosixPath(member.name)
            if ((not member.isfile() and not member.isdir()) or path.is_absolute() or '..' in path.parts or
                    '\\' in member.name or str(path) != member.name or member.name in names):
                raise RuntimeError('Invalid backup archive entry.')
            names.add(member.name)
            total += member.size
            if len(names) > MAX_FILES or total > MAX_BYTES:
                raise RuntimeError('Backup exceeds supported size.')
            if member.size > shutil.disk_usage(destination.parent).free - 1024**3:
                raise RuntimeError('Insufficient free space to validate the backup.')
            target = destination.joinpath(*path.parts)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                os.chmod(target, member.mode & 0o777)
                continue
            file_names.add(member.name)
            target.parent.mkdir(parents=True, exist_ok=True)
            with archive.extractfile(member) as content, target.open('xb') as output:
                shutil.copyfileobj(content, output)
            os.chmod(target, member.mode & 0o777)
    manifest = json.loads((destination / 'manifest.json').read_text())
    if manifest.get('schema') != 1 or set(manifest['files']) != file_names - {'manifest.json'}:
        raise RuntimeError('Backup manifest does not match its contents.')
    for name, expected in manifest['files'].items():
        if digest(destination / name) != expected:
            raise RuntimeError('Backup integrity verification failed.')
    return manifest


def apply(stage, sources, dbs):
    for destination in sources.values():
        if destination.resolve() != destination:
            raise RuntimeError('Unexpected symbolic link in recovery data paths.')
    for name in dbs:
        with (stage / 'databases' / f'{name}.dump').open('rb') as dump:
            run('runuser', '-u', 'postgres', '--', 'pg_restore', '--clean', '--if-exists', '--single-transaction', '--no-owner', '--role=unibox', '--dbname', name, source=dump)
    for name, destination in sources.items():
        incoming = stage / name
        if not incoming.is_dir():
            raise RuntimeError('Required backup data is missing.')
        # Existing files are recoverable from the independent rollback snapshot.
        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(incoming, destination)
        run('chown', '-R', 'root:unibox' if name == 'config' else 'unibox:unibox', str(destination))
        os.chmod(destination, 0o750)
    # Root-only setup credentials must not become readable by bridge processes.
    for name in ('registration.secret', 'local-account.password'):
        path = Path('/etc/unibox') / name
        if path.exists():
            os.chmod(path, 0o600)
    for destination in sources.values():
        sync_tree(destination)


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def sync_tree(root):
    for item in root.rglob('*'):
        if item.is_file():
            with item.open('rb') as source:
                os.fsync(source.fileno())
    for item in sorted((path for path in root.rglob('*') if path.is_dir()), reverse=True):
        sync_directory(item)
    sync_directory(root)


def journal_write(value):
    RECOVERY_ROOT.mkdir(mode=0o700, parents=True, exist_ok=True)
    temporary = RECOVERY_ROOT / 'pending.tmp'
    with temporary.open('w') as target:
        json.dump(value, target)
        target.flush()
        os.fsync(target.fileno())
    os.replace(temporary, RECOVERY_ROOT / 'pending.json')
    sync_directory(RECOVERY_ROOT)


def clear_journal():
    (RECOVERY_ROOT / 'pending.json').unlink(missing_ok=True)
    sync_directory(RECOVERY_ROOT)
    shutil.rmtree(RECOVERY_ROOT / 'snapshot', ignore_errors=True)


def recover_pending():
    journal = RECOVERY_ROOT / 'pending.json'
    if not journal.exists():
        return
    value = json.loads(journal.read_text())
    if value.get('schema') != 1:
        raise RuntimeError('Unsupported recovery journal. Keep the recovery snapshot.')
    sources = {}
    for name in value['roots']:
        if name == 'config':
            sources[name] = Path('/etc/unibox')
        elif name == 'data':
            sources[name] = Path('/var/lib/unibox')
        elif re.fullmatch(r'connectors/[a-z0-9_-]+/state', name):
            sources[name] = Path('/opt/unibox') / name
        else:
            raise RuntimeError('Invalid recovery data root.')
    dbs, active = value['databases'], value['services']
    if any(name != 'synapse' and not re.fullmatch(r'unibox_[a-z0-9_]+', name) for name in dbs):
        raise RuntimeError('Invalid recovery database.')
    if any(not re.fullmatch(r'unibox-[a-z0-9_-]+\.service', name) for name in active):
        raise RuntimeError('Invalid recovery service.')
    if active:
        run('systemctl', 'stop', *active)
    run('systemctl', 'start', 'postgresql')
    apply(RECOVERY_ROOT / 'snapshot', sources, dbs)
    resume(active)
    clear_journal()


def restore(source, sources, versions, dbs):
    with tempfile.TemporaryDirectory(prefix='unibox-restore-', dir='/var/tmp') as temp:
        stage = Path(temp) / 'incoming'
        stage.mkdir()
        manifest = unpack_archive(source, stage)
        if manifest['versions'] != versions or manifest['databases'] != dbs or manifest['roots'] != sorted(sources):
            raise RuntimeError('Install matching engine and connector versions before restoring this backup.')
        allowed = tuple(name + '/' for name in sources) + ('databases/',)
        if any(not name.startswith(allowed) for name in manifest['files']):
            raise RuntimeError('Unexpected backup data.')
        for name in dbs:
            run('pg_restore', '--list', str(stage / 'databases' / f'{name}.dump'))
        rollback = Path(temp) / 'rollback'
        rollback.mkdir()
        active = services()
        if active:
            run('systemctl', 'stop', *active)
        try:
            collect(rollback, sources, dbs)
        except Exception:
            resume(active)
            raise
        RECOVERY_ROOT.mkdir(mode=0o700, parents=True, exist_ok=True)
        os.chmod(RECOVERY_ROOT, 0o700)
        if (RECOVERY_ROOT / 'pending.json').exists():
            raise RuntimeError('An earlier recovery must finish before restoring again.')
        shutil.rmtree(RECOVERY_ROOT / 'snapshot', ignore_errors=True)
        try:
            shutil.move(str(rollback), RECOVERY_ROOT / 'snapshot')
            sync_tree(RECOVERY_ROOT / 'snapshot')
            journal_write({'schema': 1, 'roots': sorted(sources), 'databases': dbs, 'services': active})
        except Exception:
            resume(active)
            raise
        try:
            apply(stage, sources, dbs)
            resume(active)
        except Exception as error:
            if active:
                run('systemctl', 'stop', *active)
            try:
                apply(RECOVERY_ROOT / 'snapshot', sources, dbs)
                resume(active)
            except Exception:
                raise RuntimeError('Automatic recovery failed. The private recovery journal and snapshot were retained.') from None
            clear_journal()
            raise RuntimeError('Restore failed. The previous local data was recovered.') from error
        clear_journal()


def main():
    if len(sys.argv) == 2 and sys.argv[1] == 'recover':
        with open('/var/lock/unibox-maintenance.lock', 'w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            recover_pending()
        return
    if len(sys.argv) != 3 or sys.argv[1] not in {'backup', 'restore'}:
        raise RuntimeError('Expected backup or restore and a file path.')
    os.umask(0o077)
    path = Path(sys.argv[2])
    if not path.is_absolute() or path.suffix != '.uniboxbackup':
        raise RuntimeError('Choose a .uniboxbackup file.')
    with open('/var/lock/unibox-maintenance.lock', 'w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        recover_pending()
        source_roots, versions, dbs = roots(), fingerprint(), databases()
        if sys.argv[1] == 'restore':
            restore(path, source_roots, versions, dbs)
        else:
            active = services()
            if active:
                run('systemctl', 'stop', *active)
            try:
                with tempfile.TemporaryDirectory(prefix='unibox-backup-', dir='/var/tmp') as temp:
                    stage = Path(temp)
                    collect(stage, source_roots, dbs)
                    write_archive(stage, path, versions, dbs, source_roots)
            finally:
                resume(active)
    print(json.dumps({'ok': True}))


if __name__ == '__main__':
    try:
        main()
    except Exception:
        # Exceptions can contain SQL, filenames and sessions. Keep process output clean.
        print(json.dumps({'ok': False, 'message': 'Maintenance failed. Your backup may require matching connector versions, more disk space, or a healthy local engine.'}))
        sys.exit(1)
