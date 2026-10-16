import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('bootstrap', Path(__file__).parents[1] / 'deploy/bootstrap_board.py')
bootstrap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bootstrap)

class InstallationDefault(unittest.TestCase):
    def test_seed_and_preserve(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            target = bootstrap.seed_imu_installation(state)
            value = json.loads(target.read_text())
            default = json.loads((Path(__file__).parents[1] / 'deploy/imu-installation.default.json').read_text())
            self.assertEqual(value['imu']['mountingQuaternion'], default['imu']['mountingQuaternion'])
            self.assertEqual(value['mounting'], default['mounting'])
            self.assertFalse(value['imu']['initialized'])
            self.assertIsNone(value['imu']['quaternion'])
            self.assertEqual(value['imu']['bootId'], '')
            target.write_text('{"existing": true}')
            bootstrap.seed_imu_installation(state)
            self.assertEqual(target.read_text(), '{"existing": true}')
