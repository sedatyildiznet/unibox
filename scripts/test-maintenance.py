import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('maintenance', Path(__file__).resolve().parents[1] / 'apps/desktop/src-tauri/resources/runtime/unibox-maintenance.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ArchiveTests(unittest.TestCase):
    def test_roundtrip_and_corruption_detection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            stage = root / 'stage'
            (stage / 'data/empty').mkdir(parents=True)
            (stage / 'data/session').write_text('private test session')
            output = root / 'test.uniboxbackup'
            module.write_archive(stage, output, {'synapse': 'test'}, [], ['data'])
            restored = root / 'restored'
            restored.mkdir()
            manifest = module.unpack_archive(output, restored)
            self.assertEqual((restored / 'data/session').read_text(), 'private test session')
            self.assertTrue((restored / 'data/empty').is_dir())
            self.assertNotIn('private test session', json.dumps(manifest))
            self.assertEqual(output.stat().st_mode & 0o777, 0o600)

    def test_rejects_traversal_and_links(self):
        for name, kind in [('../outside', tarfile.REGTYPE), ('/outside', tarfile.REGTYPE), ('data/link', tarfile.SYMTYPE), ('data/hardlink', tarfile.LNKTYPE)]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive_path = root / 'bad.uniboxbackup'
                with tarfile.open(archive_path, 'w:gz') as archive:
                    entry = tarfile.TarInfo(name)
                    entry.type = kind
                    entry.linkname = '/etc/passwd'
                    archive.addfile(entry)
                destination = root / 'destination'
                destination.mkdir()
                with self.assertRaises(RuntimeError):
                    module.unpack_archive(archive_path, destination)

    def test_rejects_changed_file_digest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = {'schema': 1, 'files': {'data/value': '0' * 64}}
            archive_path = root / 'bad.uniboxbackup'
            with tarfile.open(archive_path, 'w:gz') as archive:
                for name, data in [('data/value', b'changed'), ('manifest.json', json.dumps(manifest).encode())]:
                    entry = tarfile.TarInfo(name)
                    entry.size = len(data)
                    archive.addfile(entry, io.BytesIO(data))
            with self.assertRaises(RuntimeError):
                module.unpack_archive(archive_path, root / 'destination')

    def test_version_mismatch_does_not_stop_services(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            stage = root / 'stage'
            stage.mkdir()
            output = root / 'test.uniboxbackup'
            module.write_archive(stage, output, {'synapse': 'old'}, [], [])
            with patch.object(module, 'services') as services:
                with self.assertRaises(RuntimeError):
                    module.restore(output, {}, {'synapse': 'new'}, [])
                services.assert_not_called()

    def test_failed_restore_applies_independent_rollback_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            stage = root / 'stage'
            stage.mkdir()
            output = root / 'test.uniboxbackup'
            module.write_archive(stage, output, {}, [], [])
            applications = []
            def apply(candidate, sources, dbs):
                applications.append(candidate.name)
                if candidate.name == 'incoming':
                    raise RuntimeError('Simulated restore failure')
            with patch.object(module, 'RECOVERY_ROOT', root / 'recovery'), patch.object(module, 'services', return_value=[]), patch.object(module, 'collect'), patch.object(module, 'resume'), patch.object(module, 'apply', side_effect=apply):
                with self.assertRaisesRegex(RuntimeError, 'previous local data was recovered'):
                    module.restore(output, {}, {}, [])
            self.assertEqual(applications, ['incoming', 'snapshot'])

    def test_pending_recovery_retains_snapshot_until_healthy(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(module, 'RECOVERY_ROOT', root):
                (root / 'snapshot').mkdir()
                module.journal_write({'schema': 1, 'roots': ['data'], 'databases': ['synapse'], 'services': []})
                with patch.object(module, 'run'), patch.object(module, 'apply'), patch.object(module, 'resume', side_effect=RuntimeError('offline')):
                    with self.assertRaises(RuntimeError):
                        module.recover_pending()
                self.assertTrue((root / 'pending.json').exists())
                self.assertTrue((root / 'snapshot').exists())
                with patch.object(module, 'run'), patch.object(module, 'apply') as apply, patch.object(module, 'resume'):
                    module.recover_pending()
                    apply.assert_called_once_with(root / 'snapshot', {'data': Path('/var/lib/unibox')}, ['synapse'])
                self.assertFalse((root / 'pending.json').exists())
                self.assertFalse((root / 'snapshot').exists())

    def test_recovery_recreates_directory_removed_before_interruption(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / 'snapshot/data').mkdir(parents=True)
            (root / 'snapshot/data/session').write_text('original session')
            with patch.object(module, 'run'):
                module.apply(root / 'snapshot', {'data': root / 'missing'}, [])
            self.assertEqual((root / 'missing/session').read_text(), 'original session')


unittest.main()
