"""Read-only Feetech telemetry. The only transmitted instruction is READ (0x02)."""
import os
import queue
import threading
import time

ALL_IDS = (10, 11, 12, 13, 14, 20, 21, 22, 23, 24, 30, 31, 32, 33, 34)
DEFAULT_IDS = (11, 12, 13, 14, 21, 22, 23, 24)


def parse_ids(value):
    ids = tuple(int(x.strip()) for x in value.split(','))
    if not ids or len(ids) != len(set(ids)) or any(x not in ALL_IDS for x in ids):
        raise ValueError('Servo IDs must be unique IDs in the configured robot map')
    return ids


def signed(value, bit):
    return -(value & ((1 << bit) - 1)) if value & (1 << bit) else value


def decode(data, error=0):
    if len(data) != 31:
        raise ValueError('Expected registers 40..70')
    word = lambda offset: int.from_bytes(data[offset:offset+2], 'little')
    return dict(position=signed(word(16), 15), voltage=data[22] / 10,
                temperature=data[23], currentRaw=signed(word(29), 15),
                load=signed(word(20), 10), torque=data[0], fault=data[25] | error,
                target=signed(word(27), 15), angle=None)


class ReadOnlyBus:
    def __init__(self, port):
        import serial
        self.serial = serial.Serial(port=port, baudrate=1000000, timeout=.025,
                                    write_timeout=.1, exclusive=True if os.name != 'nt' else None)

    def close(self):
        self.serial.close()

    def read_feedback(self, sid):
        if sid not in ALL_IDS:
            raise ValueError('Unknown servo ID')
        s = self.serial
        request = bytes([sid, 4, 2, 40, 31])
        s.reset_input_buffer()
        s.write(b'\xff\xff' + request + bytes([(~sum(request)) & 255]))
        deadline = time.monotonic() + .1
        received = bytearray()
        while time.monotonic() < deadline:
            received.extend(s.read(1))
            while len(received) >= 4:
                if received[:2] != b'\xff\xff':
                    del received[0]
                    continue
                length = received[3]
                if length < 2 or length > 64:
                    del received[0]
                    continue
                if len(received) < length + 4:
                    break
                frame = bytes(received[:length+4])
                del received[:length+4]
                if frame[2] != sid or (sum(frame[2:]) & 255) != 255:
                    continue
                if len(frame[5:-1]) != 31:
                    continue
                return decode(frame[5:-1], frame[4])
        raise TimeoutError('No valid feedback')


class ServoSource:
    def __init__(self, store, port, ids):
        self.store, self.port, self.ids = store, port, ids
        self.stop = threading.Event()
        self.events = queue.Queue(maxsize=1)
        self.thread = threading.Thread(target=self.run, daemon=True, name='servo-observer')

    def publish(self, rows, error=''):
        event = (dict(configuredIds=list(self.ids), servos=rows, error=error), time.monotonic())
        try:
            self.events.get_nowait()
        except queue.Empty:
            pass
        self.events.put_nowait(event)

    def run(self):
        while not self.stop.is_set():
            bus = None
            try:
                bus = ReadOnlyBus(self.port)
                while not self.stop.is_set():
                    start = time.monotonic()
                    rows = []
                    for sid in self.ids:
                        if self.stop.is_set():
                            break
                        try:
                            values = bus.read_feedback(sid)
                            rows.append(dict(id=sid, online=True, **values,
                                             received=time.monotonic()))
                        except TimeoutError:
                            rows.append(dict(id=sid, online=False))
                    now = time.monotonic()
                    for row in rows:
                        row['ageMs'] = max(0, (now-row.pop('received', now))*1000)
                    self.publish(rows)
                    self.stop.wait(max(0, .2-(time.monotonic()-start)))
            except Exception as exc:
                self.publish([dict(id=sid, online=False) for sid in self.ids],
                             'Serial unavailable: ' + type(exc).__name__)
                self.stop.wait(2)
            finally:
                if bus:
                    bus.close()

    def drain(self):
        try:
            data, timestamp = self.events.get_nowait()
        except queue.Empty:
            return
        item = self.store.sample('joints', data, timestamp=timestamp)
        item['source'] = 'hardware'
