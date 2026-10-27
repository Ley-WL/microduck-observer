"""Offline tests: no pyserial dependency or hardware access."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("capture_serial", Path(__file__).parents[1] / "tools/capture-serial.py")
capture_serial = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture_serial)


class FakePort:
    port = "test-only"
    baudrate = 1500000

    def __init__(self, chunks):
        self.chunks = iter(chunks)
        self.writes = []
        self.now = 0.0

    def read(self, _size):
        self.now += .1
        return next(self.chunks, b"")

    def write(self, data):
        self.writes.append((self.now, data))


class CaptureTests(unittest.TestCase):
    def test_receive_only_and_split_baud_marker(self):
        port = FakePort([b"boot SPSR = ", b"0x3c9\n"])
        with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
            output = Path(temp) / "capture"
            capture_serial.capture(port, output, 1., switch_baud=115200, clock=lambda: port.now)
            self.assertEqual(port.writes, [])
            self.assertEqual(port.baudrate, 115200)
            self.assertEqual((output / "boot.raw").read_bytes(), b"boot SPSR = 0x3c9\n")
            self.assertEqual(json.loads((output / "status.json").read_text(encoding="utf-8"))['phase'], 'closed')
            with self.assertRaises(FileExistsError):
                capture_serial.capture(port, output, 1., clock=lambda: port.now)

    def test_interrupt_is_opt_in_bounded_and_requires_marker(self):
        for chunks, expected in [([b"ordinary boot"], False), ([b"SPSR = 0x3c9"], True)]:
            port = FakePort(chunks)
            with tempfile.TemporaryDirectory() as temp, contextlib.redirect_stdout(io.StringIO()):
                capture_serial.capture(port, Path(temp) / "capture", 10., interrupt_boot=True, clock=lambda: port.now)
            self.assertEqual(bool(port.writes), expected)
            self.assertTrue(all(at < 8.1 and data == b" " for at, data in port.writes))

    def test_read_failure_is_reported_and_propagated(self):
        port = FakePort([])
        def fail(_size):
            raise OSError("test disconnection")
        port.read = fail
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "capture"
            with self.assertRaisesRegex(OSError, "test disconnection"):
                capture_serial.capture(port, output, 1., clock=lambda: port.now)
            self.assertEqual(json.loads((output / "status.json").read_text(encoding="utf-8"))["phase"], "failed")


if __name__ == "__main__":
    unittest.main()
