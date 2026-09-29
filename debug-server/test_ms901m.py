import math
import struct
import time
import unittest
from pathlib import Path
from ms901m import FrameParser, Ms901mSource, decode_quaternion, decode_raw, range_query
from server import Simulator


def frame(head, kind, payload):
    data = bytes((0x55, head, kind, len(payload))) + payload
    return data + bytes((sum(data) & 255,))


class Ms901mTests(unittest.TestCase):
    def test_fragmented_real_capture_and_checksum(self):
        capture = (Path(__file__).resolve().parents[1] / 'docs/实测记录/附件/IMU/2026-09-29/UART4-115200-10秒.bin').read_bytes()
        parser = FrameParser()
        frames = []
        for i in range(0, len(capture), 7):
            frames.extend(parser.feed(capture[i:i+7]))
        self.assertEqual(len(frames), 7961)
        self.assertEqual(parser.bad_checksums, 0)
        self.assertEqual({kind for head, kind, p in frames}, {1, 2, 3, 6})

    def test_corruption_resynchronizes_and_buffer_is_bounded(self):
        good = frame(0x55, 2, struct.pack('<4h', 32767, 0, 0, 0))
        bad = good[:-1] + bytes((good[-1] ^ 1,))
        parser = FrameParser()
        result = parser.feed(b'\x55\x55\x02\xff' + bad + good)
        self.assertEqual(len(result), 1)
        self.assertEqual(parser.bad_checksums, 1)
        parser.feed(b'\x12' * 10000)
        self.assertLess(len(parser.buffer), 4)

    def test_vendor_units_and_quaternion_order(self):
        self.assertEqual(decode_quaternion(struct.pack('<4h', 16384, -16384, 16384, -16384)), [-.5, .5, -.5, .5])
        raw = decode_raw(struct.pack('<6h', 8192, -8192, 0, 16384, 0, -16384), 4, 2000)
        self.assertAlmostEqual(raw['accel'][0], 9.80665)
        self.assertAlmostEqual(raw['accel'][1], -9.80665)
        self.assertAlmostEqual(raw['gyro'][0], math.radians(1000))
        with self.assertRaises(ValueError):
            decode_raw(bytes(12), None, 2000)
        with self.assertRaises(ValueError):
            decode_quaternion(bytes(7))

    def test_register_query_and_unknown_range_suppression(self):
        self.assertEqual(range_query(3), bytes.fromhex('55af83010088'))
        with self.assertRaises(ValueError):
            range_query(0)
        store = Simulator('hardware')
        source = Ms901mSource(store)
        raw = struct.pack('<6h', 0, 0, 8192, 0, 0, 0)
        source.process(0x55, 3, raw, time.monotonic())
        source.drain()
        self.assertNotIn('imu.raw', store.latest)
        source.process(0xAF, 3, b'\x03', time.monotonic())
        source.process(0xAF, 4, b'\x01', time.monotonic())
        source.process(0x55, 3, raw, time.monotonic()-2)
        source.process(0x55, 2, struct.pack('<4h', 32767, 0, 0, 0), time.monotonic()-2)
        source.drain()
        self.assertEqual(source.health()['state'], 'unavailable')
        self.assertEqual(source.health()['ranges'], {'accelG':4, 'gyroDps':2000})
        self.assertAlmostEqual(store.latest['imu.raw']['data']['accel'][2], 9.80665)
        self.assertNotIn('pose', store.latest)


if __name__ == '__main__':
    unittest.main()
