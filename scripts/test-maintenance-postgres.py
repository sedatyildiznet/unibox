"""Run against an isolated PostgreSQL CI service, never a user's database."""
import importlib.util
from pathlib import Path
import tempfile
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('maintenance', Path(__file__).resolve().parents[1] / 'apps/desktop/src-tauri/resources/runtime/unibox-maintenance.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
original_run = m.run

def ci_run(*args, **kwargs):
    if args[:4] == ('runuser', '-u', 'postgres', '--'):
        args = args[4:]
    if args[0] in {'chown', 'systemctl'}:
        return ''  # CI has no application OS account; PostgreSQL roles are real.
    return original_run(*args, **kwargs)

m.run = ci_run
DB = 'unibox_maintenance_ci'
ci_run('psql', '-v', 'ON_ERROR_STOP=1', '-c', 'CREATE ROLE unibox;')
ci_run('createdb', '--owner=unibox', DB)
try:
    ci_run('psql', '-v', 'ON_ERROR_STOP=1', '-d', DB, '-c', "SET ROLE unibox; CREATE TABLE messages (body text); INSERT INTO messages VALUES ('original');")
    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp)
        source = root / 'live-data'
        source.mkdir()
        (source / 'session').write_text('original-session')
        stage = root / 'snapshot'
        stage.mkdir()
        m.RECOVERY_ROOT = root / 'recovery'
        sources = {'data': source}
        m.collect(stage, sources, [DB])
        archive = root / 'snapshot.uniboxbackup'
        m.write_archive(stage, archive, {}, [DB], sources)
        ci_run('psql', '-v', 'ON_ERROR_STOP=1', '-d', DB, '-c', "UPDATE messages SET body='newer';")
        (source / 'session').write_text('newer-session')
        with patch.object(m, 'services', return_value=[]), patch.object(m, 'resume'):
            m.restore(archive, sources, {}, [DB])
        assert ci_run('psql', '-At', '-d', DB, '-c', 'SELECT body FROM messages') == 'original'
        assert (source / 'session').read_text() == 'original-session'

        # Fail health validation after applying the archive. The independent snapshot
        # must recover both the database and session files from immediately before restore.
        ci_run('psql', '-v', 'ON_ERROR_STOP=1', '-d', DB, '-c', "UPDATE messages SET body='latest';")
        (source / 'session').write_text('latest-session')
        with patch.object(m, 'services', return_value=[]), patch.object(m, 'resume', side_effect=[RuntimeError('health failure'), None]):
            try:
                m.restore(archive, sources, {}, [DB])
                raise AssertionError('Restore unexpectedly succeeded')
            except RuntimeError as error:
                assert 'previous local data was recovered' in str(error)
        assert ci_run('psql', '-At', '-d', DB, '-c', 'SELECT body FROM messages') == 'latest'
        assert (source / 'session').read_text() == 'latest-session'
        # Simulate a process stopping after the pre-restore journal is durable.
        pending = m.RECOVERY_ROOT / 'snapshot'
        pending.mkdir(parents=True)
        m.collect(pending, sources, [DB])
        m.journal_write({'schema': 1, 'roots': ['data'], 'databases': [DB], 'services': []})
        ci_run('psql', '-v', 'ON_ERROR_STOP=1', '-d', DB, '-c', "UPDATE messages SET body='interrupted';")
        (source / 'session').write_text('interrupted-session')
        original_apply = m.apply
        with patch.object(m, 'apply', side_effect=lambda candidate, ignored, dbs: original_apply(candidate, sources, dbs)), patch.object(m, 'resume'):
            m.recover_pending()
        assert ci_run('psql', '-At', '-d', DB, '-c', 'SELECT body FROM messages') == 'latest'
        assert (source / 'session').read_text() == 'latest-session'
        assert not (m.RECOVERY_ROOT / 'pending.json').exists()
    print('Real PostgreSQL backup, restore and recovery checks passed.')
finally:
    ci_run('dropdb', '--if-exists', DB)
