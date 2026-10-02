"""Latest IMU snapshot shared by acquisition thread and serial process.

No feeder thread or backlog; reading does not remove the latest sample.
"""
import json
import ctypes


class ImuMailbox:
    def __init__(self, context):
        self.buffer = context.RawArray('B', 8192)
        self.length = context.RawValue('I', 0)
        self.lock = context.Lock()

    def update(self, samples):
        with self.lock:
            current = self._read()
            current.update(samples)
            payload = json.dumps(current, separators=(',', ':')).encode('utf-8')
            if len(payload) >= len(self.buffer):
                raise ValueError('IMU snapshot too large')
            ctypes.memmove(ctypes.addressof(self.buffer), payload, len(payload))
            self.length.value = len(payload)

    def _read(self):
        payload = ctypes.string_at(ctypes.addressof(self.buffer), self.length.value)
        return json.loads(payload) if payload else {}

    def snapshot(self):
        with self.lock:
            return self._read()
