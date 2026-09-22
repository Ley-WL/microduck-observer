import math
import struct
import time
import unittest
from unittest.mock import patch
from fastapi import HTTPException
from bno085 import BNO085, decode_reports
from hardware import HardwareSource
from server import Simulator, Scenario
import server


class HardwareTests(unittest.TestCase):
    def test_i2c_header_probe_continuation_bit(self):
        reader = BNO085.__new__(BNO085)
        reader.fd = 7
        payload = bytes([1, 1, 3, 0]) + struct.pack('<hhh', 0, 0, 2511)
        header = struct.pack('<HBB', 14, 3, 1)
        continued = struct.pack('<HBB', 0x8000 | 14, 3, 1) + payload
        with patch('bno085.os.read', side_effect=[header, continued]):
            self.assertEqual(reader.read(), (3, payload))

    def test_decode_real_fixed_point_reports(self):
        payload = bytes([0xfb, 0, 0, 0, 0])
        payload += bytes([1, 1, 3, 0]) + struct.pack('<hhh', -256, 512, 2511)
        payload += bytes([5, 2, 2, 0]) + struct.pack('<hhhhh', 0, 0, 0, 16384, 0)
        values = list(decode_reports(payload))
        self.assertEqual(values[0], ('accel', [-1, 2, 2511/256], 3))
        self.assertEqual(values[1], ('quaternion', [0, 0, 0, 1], 2))

    def test_truncated_or_unknown_reports_are_rejected(self):
        for payload in (b'\x05\x00', b'\xff', b'\xfb\x00'):
            with self.assertRaises(ValueError):
                list(decode_reports(payload))

    def test_hardware_orientation_never_becomes_robot_pose(self):
        store = Simulator('hardware')
        source = HardwareSource(store)
        stamp = time.monotonic() - .2
        source.put('quaternion', ([0,0,0,1], 3), stamp)
        source.drain()
        self.assertNotIn('pose', store.latest)
        sample = store.stamp(store.latest['imu.orientation'])
        self.assertEqual(sample['source'], 'hardware')
        self.assertEqual(sample['data']['frame'], 'sensor')
        self.assertFalse(sample['data']['mountingCalibrated'])
        self.assertGreaterEqual(sample['ageMs'], 200)

    def test_missing_accelerometer_does_not_become_fresh(self):
        store = Simulator('hardware')
        source = HardwareSource(store)
        source.put('accel', ([0,0,9.81], 3), time.monotonic()-2)
        source.put('gyro', ([0,0,0], 3))
        source.drain()
        self.assertFalse(store.latest['imu.raw']['valid'])
        self.assertGreaterEqual(store.stamp(store.latest['imu.raw'])['ageMs'], 2000)

    def test_simulation_controls_disabled_in_hardware_mode(self):
        with patch.object(server, 'SOURCE', 'hardware'):
            with self.assertRaises(HTTPException) as error:
                server.scenario(Scenario(name='motion'))
            self.assertEqual(error.exception.status_code, 404)


if __name__ == '__main__':
    unittest.main()
