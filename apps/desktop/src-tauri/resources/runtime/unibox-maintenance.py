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
    for name in dbs:
        with (stage / 'databases' / f'{name}.dump').open('rb') as dump:
            run('runuser', '-u', 'postgres', '--', 'pg_restore', '--clean', '--if-exists', '--single-transaction', '--no-owner', '--role=unibox', '--dbname', name, source=dump)
    for name, destination in sources.items():
        incoming = stage / name
        if not incoming.is_dir():
            raise RuntimeError('Required backup data is missing.')
        # Existing files are recoverable from the independent rollback snapshot.
        shutil.rmtree(destination)
        shutil.copytree(incoming, destination)
        run('chown', '-R', 'root:unibox' if name == 'config' else 'unibox:unibox', str(destination))
        os.chmod(destination, 0o750)
    # Root-only setup credentials must not become readable by bridge processes.
    for name in ('registration.secret', 'local-account.password'):
        path = Path('/etc/unibox') / name
        if path.exists():
            os.chmod(path, 0o600)


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
        try:
            apply(stage, sources, dbs)
            resume(active)
        except Exception as error:
            # Stop partially recovered writers before restoring the previous databases.
            if active:
                run('systemctl', 'stop', *active)
            try:
                apply(rollback, sources, dbs)
                resume(active)
            except Exception:
                recovery = Path('/var/tmp') / ('unibox-recovery-' + os.urandom(8).hex())
                rollback.rename(recovery)
                os.chmod(recovery, 0o700)
                raise RuntimeError('Automatic recovery failed. A private recovery snapshot was retained in /var/tmp.') from None
            raise RuntimeError('Restore failed. The previous local data was recovered.') from error


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in {'backup', 'restore'}:
        raise RuntimeError('Expected backup or restore and a file path.')
    os.umask(0o077)
    path = Path(sys.argv[2])
    if not path.is_absolute() or path.suffix != '.uniboxbackup':
        raise RuntimeError('Choose a .uniboxbackup file.')
    with open('/var/lock/unibox-maintenance.lock', 'w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
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
