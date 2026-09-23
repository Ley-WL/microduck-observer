"""Read-only Feetech telemetry. Only READ (0x02) and SYNC_READ (0x82) are transmitted."""
import os
import queue
import multiprocessing
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
            # Wait for the first byte, then consume the buffered remainder in one
            # read. Never request an arbitrary full frame: a partial response
            # would otherwise block until the serial timeout.
            received.extend(s.read(max(1, min(s.in_waiting, 128))))
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


    def read_feedback_many(self, ids):
        """One broadcast read, independently validate each ID; never write registers."""
        if not ids or len(set(ids)) != len(ids) or any(sid not in ALL_IDS for sid in ids):
            raise ValueError('Invalid sync-read IDs')
        s = self.serial
        request = bytes([254, len(ids)+4, 0x82, 40, 31, *ids])
        s.reset_input_buffer()
        s.write(b'\xff\xff' + request + bytes([(~sum(request)) & 255]))
        started = time.monotonic()
        deadline = started + .02
        received = bytearray()
        out = {}
        timeout = s.timeout
        s.timeout = .002
        try:
            while time.monotonic() < deadline and len(out) < len(ids):
                received.extend(s.read(max(1, min(s.in_waiting, 1024))))
                while len(received) >= 4:
                    if received[:2] != b'\xff\xff' or not 2 <= received[3] <= 64:
                        del received[0]
                        continue
                    length = received[3] + 4
                    if len(received) < length:
                        break
                    frame = bytes(received[:length]); del received[:length]
                    sid = frame[2]
                    if sid not in ids or sid in out or (sum(frame[2:]) & 255) != 255 or len(frame[5:-1]) != 31:
                        continue
                    now = time.monotonic()
                    out[sid] = dict(**decode(frame[5:-1], frame[4]),
                                    received=now, readMs=(now-started)*1000)
        finally:
            s.timeout = timeout
        return out


class ServoPoller:
    """Independent read-only worker; no web/IMU objects cross process boundary."""
    def __init__(self, port, ids, events, stop, period=.02):
        self.port, self.ids, self.events, self.stop, self.period = port, ids, events, stop, period

    def publish(self, rows, error='', scan_ms=None):
        event = (dict(configuredIds=list(self.ids), servos=rows, error=error,
                      scanMs=scan_ms, targetHz=1/self.period, readMode='sync-read'), time.monotonic())
        try:
            self.events.put_nowait(event)
        except queue.Full:
            try:
                self.events.get_nowait()
            except queue.Empty:
                return  # Feeder has not made the occupied slot readable yet.
            try:
                self.events.put_nowait(event)
            except queue.Full:
                pass  # Never stall the hardware reader behind a slow consumer.

    def run(self):
        while not self.stop.is_set():
            bus = None
            try:
                bus = ReadOnlyBus(self.port)
                retry_after = {}
                while not self.stop.is_set():
                    start = time.monotonic()
                    active = [sid for sid in self.ids if time.monotonic() >= retry_after.get(sid, 0)]
                    feedback = bus.read_feedback_many(active) if active else {}
                    rows = []
                    for sid in self.ids:
                        if sid in feedback:
                            rows.append(dict(id=sid, online=True, **feedback[sid]))
                        else:
                            if sid in active:
                                retry_after[sid] = time.monotonic() + 1
                            rows.append(dict(id=sid, online=False))
                    now = time.monotonic()
                    for row in rows:
                        row['ageMs'] = max(0, (now-row.pop('received', now))*1000)
                    self.publish(rows, scan_ms=(now-start)*1000)
                    self.stop.wait(max(0, self.period-(time.monotonic()-start)))
            except Exception as exc:
                self.publish([dict(id=sid, online=False) for sid in self.ids],
                             'Serial unavailable: ' + type(exc).__name__)
                self.stop.wait(2)
            finally:
                if bus:
                    bus.close()


def run_servo_poller(port, ids, events, stop):
    # Exit never waits for a final unread telemetry packet to flush.
    events.cancel_join_thread()
    ServoPoller(port, ids, events, stop).run()


class ServoSource:
    def __init__(self, store, port, ids):
        self.store = store
        context = multiprocessing.get_context('spawn')
        self.stop = context.Event()
        self.events = context.Queue(maxsize=1)
        # Keep start/join compatibility with the existing lifespan owner.
        self.thread = context.Process(target=run_servo_poller,
            args=(port, ids, self.events, self.stop), daemon=True, name='servo-observer')

    def close(self):
        self.stop.set()
        self.thread.join(2)
        if self.thread.is_alive():
            self.thread.terminate()
            self.thread.join(2)
        self.events.close()

    def drain(self):
        try:
            data, timestamp = self.events.get_nowait()
        except queue.Empty:
            return
        item = self.store.sample('joints', data, timestamp=timestamp)
        item['source'] = 'hardware'
