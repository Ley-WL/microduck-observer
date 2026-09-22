"""Bounded thread-to-async bridge; blocking I2C never runs in the web loop."""
import math
import queue
import threading
import time
from collections import Counter
from bno085 import BNO085, decode_reports


class HardwareSource:
    def __init__(self, store):
        self.store = store
        self.events = queue.Queue(maxsize=512)
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.run, name='bno085-reader', daemon=True)
        self.counts = Counter()
        self.errors = 0
        self.unparsed = 0
        self.dropped = 0
        self.product_id = None
        self.values = {}
        self.started = time.monotonic()

    def put(self, kind, value, timestamp=None):
        event = (kind, value, timestamp or time.monotonic())
        try:
            self.events.put_nowait(event)
        except queue.Full:
            self.dropped += 1
            try:
                self.events.get_nowait()
            except queue.Empty:
                pass
            self.events.put_nowait(event)

    def run(self):
        while not self.stop.is_set():
            reader = None
            try:
                reader = BNO085()
                self.product_id = reader.start(self.stop)
                self.put('log', ('INFO', 'BNO085 已连接 · /dev/i2c-4 @ 0x4B；安装方向未校准'))
                consecutive_errors = 0
                while not self.stop.is_set():
                    try:
                        packet = reader.read()
                        if packet and packet[0] in (3, 4):
                            received = time.monotonic()
                            for name, values, accuracy in decode_reports(packet[1]):
                                self.counts[name] += 1
                                self.put(name, (values, accuracy), received)
                        consecutive_errors = 0
                    except OSError:
                        self.errors += 1
                        consecutive_errors += 1
                        if consecutive_errors >= 10:
                            raise
                        self.stop.wait(.01)
                    except ValueError:
                        self.unparsed += 1
                    self.stop.wait(.001)
            except Exception as exc:
                self.put('log', ('ERROR', f'IMU 读取失败，将在 3 秒后重试：{exc}'))
            finally:
                if reader:
                    reader.close()
            self.stop.wait(3)

    def drain(self):
        for _ in range(512):
            try:
                kind, payload, timestamp = self.events.get_nowait()
            except queue.Empty:
                break
            if kind == 'log':
                level, message = payload
                self.store.log(level, 'imu', message)
                continue
            values, accuracy = payload
            if kind == 'quaternion':
                valid = abs(math.hypot(*values) - 1) < .05
                self.store.sample('imu.orientation', dict(frame='sensor', quaternion=values,
                    accuracy=accuracy, calibrationId=None, mountingCalibrated=False,
                    timestampBasis='host_receive'), valid, timestamp)
            else:
                self.values[kind] = (values, timestamp, accuracy)
                if kind == 'gyro' and 'accel' in self.values:
                    accel, accel_time, accel_accuracy = self.values['accel']
                    self.store.sample('imu.raw', dict(frame='sensor', gyro=values, accel=accel,
                        gyroAccuracy=accuracy, accelAccuracy=accel_accuracy,
                        timestampBasis='host_receive'), timestamp - accel_time < .5,
                        min(timestamp, accel_time))

    def health(self):
        elapsed = max(.001, time.monotonic()-self.started)
        item = self.store.latest.get('imu.orientation')
        age = self.store.stamp(item)['ageMs'] if item else None
        return dict(device='/dev/i2c-4', address='0x4B', productId=self.product_id,
                    state='streaming' if age is not None and age < 1500 else 'unavailable',
                    sampleAgeMs=age, counts=dict(self.counts),
                    observedHz={k: round(v/elapsed, 2) for k, v in self.counts.items()},
                    ioErrors=self.errors, unparsedReports=self.unparsed, droppedEvents=self.dropped,
                    mountingCalibrated=False)
