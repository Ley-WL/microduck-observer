import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('loop', Path(__file__).parents[1] / 'deploy/check_uart_loopback.py')
loop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(loop)


class LoopbackTests(unittest.TestCase):
    def test_exact_payload_and_binary_values(self):
        data = b'UART2-LOOPBACK-' + bytes([0]) + bytes(range(32))
        self.assertEqual(len(data), 48)
        self.assertTrue(loop.compare(data, data)['exactMatch'])

    def test_missing_extra_and_corrupted_data_are_not_success(self):
        data = bytes(range(48))
        for received in [b'', data[:-1], b'\xfe' + data, data[:-1] + b'\xff']:
            self.assertFalse(loop.compare(data, received)['exactMatch'])


if __name__ == '__main__':
    unittest.main()
