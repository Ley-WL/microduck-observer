import importlib.util
import json
import os
from pathlib import Path
import stat
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('tof_prepare', Path(__file__).parents[1] / 'deploy/tof_prepare.py')
tof = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tof)


class TofPermissionsTests(unittest.TestCase):
    def test_regular_file_is_not_chowned_or_chmodded(self):
        with tempfile.NamedTemporaryFile() as f, patch.object(os, 'fchown') as chown:
            with self.assertRaisesRegex(RuntimeError, 'I2C3'):
                tof.fix_permissions(f.name)
            chown.assert_not_called()

    def test_symlink_is_not_followed(self):
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder) / 'target'
            target.write_text('unchanged')
            link = Path(folder) / 'link'
            link.symlink_to(target)
            with self.assertRaises(OSError):
                tof.fix_permissions(link)
            self.assertEqual(target.read_text(), 'unchanged')

    def test_other_character_device_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, '89:3'):
            tof.fix_permissions('/dev/null')

    def test_expected_device_has_only_its_open_descriptor_changed(self):
        import grp
        with tempfile.NamedTemporaryFile() as f:
            info = SimpleNamespace(st_mode=stat.S_IFCHR, st_rdev=os.makedev(89, 3))
            with patch.object(os, 'fstat', return_value=info), patch.object(os, 'fchown') as chown, \
                    patch.object(os, 'fchmod') as chmod, patch.object(grp, 'getgrnam', return_value=SimpleNamespace(gr_gid=321)):
                tof.fix_permissions(f.name)
                fd = chown.call_args.args[0]
                chown.assert_called_once_with(fd, 0, 321)
                chmod.assert_called_once_with(fd, 0o660)

    def test_missing_bus_writes_nonfatal_diagnostic(self):
        with tempfile.TemporaryDirectory() as folder:
            status = Path(folder) / 'status.json'
            with patch.object(tof, 'BUS', Path(folder) / 'missing'), \
                    patch.object(tof.subprocess, 'run'), \
                    patch.object(tof, 'fix_permissions', side_effect=FileNotFoundError('I2C3 absent')):
                result = tof.prepare(status, wait_seconds=0)
            self.assertFalse(result['ready'])
            self.assertIn('I2C3 absent', result['error'])
            self.assertFalse(json.loads(status.read_text())['sensorCommandsSent'])


if __name__ == '__main__':
    unittest.main()
