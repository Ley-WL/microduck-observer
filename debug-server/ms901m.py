"""ALIENTEK MS901M UART telemetry. Queries ranges; never writes configuration.

Protocol reference: ALIENTEK atk_ms901m.c V1.0 (2022-06-21), mirrored at
github.com/jyyy3901/esp-drone-ms901m under hardware/ATK-IMU901模块资料（新资料）.
"""
import math
import os
import queue
import struct
import time
from collections import Counter

from hardware import HardwareSource


class FrameParser:
    def __init__(self):
        self.buffer = bytearray()
        self.bad_checksums = 0
        self.discarded = 0

    def feed(self, data):
        self.buffer.extend(data)
        frames = []
        while len(self.buffer) >= 4:
            b = self.buffer
            if b[0] != 0x55 or b[1] not in (0x55, 0xAF) or b[3] > 64:
                del b[0]
                self.discarded += 1
                continue
            size = b[3] + 5
            if len(b) < size:
                break
            if sum(b[:size-1]) & 255 != b[size-1]:
                self.bad_checksums += 1
                del b[0]
                continue
            frames.append((b[1], b[2], bytes(b[4:size-1])))
            del b[:size]
        return frames


def range_query(register):
    if register not in (3, 4):
        raise ValueError('Only read-only range queries are allowed')
    command = bytes((0x55, 0xAF, register | 0x80, 1, 0))
    return command + bytes((sum(command) & 255,))


def decode_quaternion(payload):
    if len(payload) != 8:
        raise ValueError('Invalid quaternion length')
    w, x, y, z = (v / 32768 for v in struct.unpack('<4h', payload))
    return [x, y, z, w]


def decode_raw(payload, accel_g, gyro_dps):
    if len(payload) != 12:
        raise ValueError('Invalid acceleration/gyro length')
    if accel_g not in (2, 4, 8, 16) or gyro_dps not in (250, 500, 1000, 2000):
        raise ValueError('Ranges must be read from the sensor')
    values = struct.unpack('<6h', payload)
    return dict(accel=[v / 32768 * accel_g * 9.80665 for v in values[:3]],
                gyro=[v / 32768 * math.radians(gyro_dps) for v in values[3:]])


class Ms901mSource(HardwareSource):
    def __init__(self, store, port=None):
        super().__init__(store)
        self.port = port or os.environ.get('MICRODUCK_IMU_PORT', '/dev/ttyS4')
        self.thread.name = 'ms901m-reader'
        self.accel_g = None
        self.gyro_dps = None
        self.checksum_errors = 0
        self.frames = Counter()

    def policy_sample(self, kind, value, received):
        if kind == 'quaternion':
            return ('imu.orientation', dict(quaternion=value), abs(math.hypot(*value)-1)<.05, received)
        if kind == 'raw':
            return ('imu.raw', value, True, received)
        return None

    def process(self, head, kind, payload, received):
        if head == 0xAF:
            if kind in (3, 4) and len(payload) == 1 and payload[0] < 4:
                if kind == 3:
                    self.gyro_dps = (250, 500, 1000, 2000)[payload[0]]
                else:
                    self.accel_g = (2, 4, 8, 16)[payload[0]]
            return
        self.frames[f'{kind:02x}'] += 1
        if kind == 2:
            values = decode_quaternion(payload)
            self.counts['quaternion'] += 1
            self.put('quaternion', values, received)
        elif kind == 3:
            if len(payload) != 12:
                raise ValueError('Invalid raw length')
            if self.accel_g is not None and self.gyro_dps is not None:
                values = decode_raw(payload, self.accel_g, self.gyro_dps)
                self.counts['accel'] += 1
                self.counts['gyro'] += 1
                self.put('raw', values, received)

    def run(self):
        import serial
        while not self.stop.is_set():
            try:
                self.accel_g = self.gyro_dps = None
                parser = FrameParser()
                with serial.Serial(self.port, 115200, timeout=.1, exclusive=True) as reader:
                    reader.reset_input_buffer()
                    last_query = 0
                    last_frame = time.monotonic()
                    self.put('log', ('INFO', f'MS901M 已连接 · {self.port} · 115200；安装方向未校准'))
                    while not self.stop.is_set():
                        now = time.monotonic()
                        if now - last_query > 2 and (self.accel_g is None or self.gyro_dps is None):
                            reader.write(range_query(3))
                            reader.write(range_query(4))
                            last_query = now
                        data = reader.read(min(reader.in_waiting, 4096) or 1)
                        received = time.monotonic()
                        before = parser.bad_checksums
                        frames = parser.feed(data)
                        self.checksum_errors += parser.bad_checksums - before
                        for head, kind, payload in frames:
                            last_frame = received
                            try:
                                self.process(head, kind, payload, received)
                            except ValueError:
                                self.unparsed += 1
                        if received - last_frame > 3:
                            raise OSError('IMU 数据流中断')
            except Exception as exc:
                self.errors += 1
                self.put('log', ('ERROR', f'MS901M 读取失败，3秒后重试：{exc}'))
            self.stop.wait(3)

    def drain(self):
        latest = {}
        for _ in range(512):
            try:
                kind, value, received = self.events.get_nowait()
            except queue.Empty:
                break
            if kind == 'log':
                self.store.log(value[0], 'imu', value[1])
                continue
            latest[kind] = (value, received)
        # The stream sends the latest sample, so do not repeatedly materialize
        # obsolete samples from a batch while delaying joints and WebSockets.
        for kind, (value, received) in latest.items():
            metadata = dict(frame='sensor', device='MS901M', timestampBasis='host_receive')
            if kind == 'quaternion':
                self.store.sample('imu.orientation', dict(**metadata, quaternion=value,
                    accuracy=None, calibrationId=None, mountingCalibrated=False),
                    abs(math.hypot(*value) - 1) < .05, received)
            elif kind == 'raw':
                self.store.sample('imu.raw', dict(**metadata, **value, accelAccuracy=None,
                    gyroAccuracy=None, accelUnit='m/s²', gyroUnit='rad/s'), True, received)

    def health(self):
        result = super().health()
        orientation = self.store.latest.get('imu.orientation')
        if orientation and not orientation['valid']:
            result['state'] = 'unavailable'
        result.update(device=self.port, model='MS901M', address=None, baud=115200,
                      productId=None, ranges=dict(accelG=self.accel_g, gyroDps=self.gyro_dps),
                      checksumErrors=self.checksum_errors, frames=dict(self.frames))
        result['rangeState'] = 'confirmed' if self.accel_g and self.gyro_dps else 'unknown'
        return result
