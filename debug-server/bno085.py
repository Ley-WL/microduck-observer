"""Linux i2c-dev SHTP reader derived from the verified standalone diagnostic.

No motor access. One owner, no reset command, only sensor feature reports.
"""
import os
import struct
import time
from collections import defaultdict

REPORTS = {1: ('accel', 10, 256), 2: ('gyro', 10, 512), 5: ('quaternion', 14, 16384)}


def decode_reports(payload):
    offset = 0
    while offset < len(payload):
        rid = payload[offset]
        if rid in (0xfb, 0xfa):
            if offset + 5 > len(payload):
                raise ValueError('Truncated timestamp')
            offset += 5
            continue
        if rid not in REPORTS:
            raise ValueError(f'Unsupported report 0x{rid:02x}')
        name, size, scale = REPORTS[rid]
        if offset + size > len(payload):
            raise ValueError('Truncated sensor report')
        values = struct.unpack_from('<hhhh' if rid == 5 else '<hhh', payload, offset + 4)
        yield name, [value / scale for value in values], payload[offset + 2] & 3
        offset += size


class BNO085:
    def __init__(self, device='/dev/i2c-4', address=0x4b):
        import fcntl
        self.fd = os.open(device, os.O_RDWR)
        self.seq = defaultdict(int)
        self.enabled = []
        try:
            fcntl.flock(self.fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            fcntl.ioctl(self.fd, 0x0703, address)
        except Exception:
            os.close(self.fd)
            raise

    def send(self, channel, payload):
        header = struct.pack('<HBB', len(payload) + 4, channel, self.seq[channel])
        self.seq[channel] = (self.seq[channel] + 1) % 256
        packet = header + payload
        if os.write(self.fd, packet) != len(packet):
            raise OSError('Short I2C write')

    def read(self):
        header = os.read(self.fd, 4)
        if len(header) != 4:
            raise OSError('Short I2C header')
        length = struct.unpack_from('<H', header)[0] & 0x7fff
        if length == 0:
            return None
        if length < 4 or length > 4096:
            raise ValueError(f'Invalid SHTP length {length}')
        packet = os.read(self.fd, length)
        if len(packet) != length:
            raise OSError('Short I2C packet')
        # BNO085 repeats the header with the continuation bit after the 4-byte
        # header probe. This is expected on this I2C transport, as verified by
        # scripts/bno085_diagnostic.py; it does not invalidate this full read.
        return packet[2], packet[4:]

    def start(self, stop):
        until = time.monotonic() + .3
        while time.monotonic() < until and not stop.is_set():
            self.read()
            stop.wait(.002)
        self.send(2, bytes([0xf9, 0]))
        product = None
        until = time.monotonic() + 2
        while time.monotonic() < until and not stop.is_set():
            packet = self.read()
            if packet and packet[0] == 2 and packet[1] and packet[1][0] == 0xf8:
                product = packet[1].hex()
                break
            stop.wait(.002)
        if not product:
            raise OSError('BNO085 product ID response timed out')
        for rid in REPORTS:
            self.send(2, struct.pack('<BBBHIII', 0xfd, rid, 0, 0, 20000, 0, 0))
            self.enabled.append(rid)
            stop.wait(.02)
        return product

    def close(self):
        for rid in self.enabled:
            try:
                self.send(2, struct.pack('<BBBHIII', 0xfd, rid, 0, 0, 0, 0, 0))
            except OSError:
                pass
        os.close(self.fd)
