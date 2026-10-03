"""Sync-read telemetry with explicit calibration in the same serial-owning process."""
import concurrent.futures
import os
import queue
import multiprocessing
from imu_mailbox import ImuMailbox
import time
from collections import deque

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
                accelerationRaw=data[1], goalPositionRaw=signed(word(2), 15),
                goalCurrentRaw=signed(word(4), 15), speedLimitRaw=signed(word(6), 15),
                torqueLimitRaw=word(8), kpRaw=data[10], kdRaw=data[11], kiRaw=data[12],
                target=signed(word(27), 15), angle=None)


class ReadOnlyBus:
    def __init__(self, port):
        import serial
        self.serial = serial.Serial(port=port, baudrate=1000000, timeout=.025,
                                    write_timeout=.1, exclusive=True if os.name != 'nt' else None)
        self.trace = deque(maxlen=8)
        self.last_read = None

    def record_write(self, packet):
        if hasattr(self, 'trace'):
            self.trace.append(dict(kind='write', mono=time.monotonic(), txHex=packet.hex()))

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
                if (sum(frame[2:]) & 255) != 255:
                    del received[0]
                    continue
                del received[:length+4]
                if frame[2] != sid:
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
        packet = b'\xff\xff' + request + bytes([(~sum(request)) & 255])
        before_clear = s.in_waiting
        s.reset_input_buffer()
        s.write(packet)
        started = time.monotonic()
        deadline = started + .02
        received = bytearray()
        out = {}
        raw = bytearray()
        checksum_errors = 0
        rejected_frames = []
        discarded_bytes = 0
        timeout = s.timeout
        s.timeout = .002
        try:
            while time.monotonic() < deadline and len(out) < len(ids):
                chunk = s.read(max(1, min(s.in_waiting, 1024)))
                raw.extend(chunk)
                received.extend(chunk)
                while len(received) >= 4:
                    if received[:2] != b'\xff\xff' or not 2 <= received[3] <= 64:
                        del received[0]
                        discarded_bytes += 1
                        continue
                    length = received[3] + 4
                    if len(received) < length:
                        break
                    frame = bytes(received[:length])
                    sid = frame[2]
                    if (sum(frame[2:]) & 255) != 255:
                        checksum_errors += 1
                        rejected_frames.append(dict(id=sid, reason='checksum', hex=frame.hex()))
                        del received[0]
                        discarded_bytes += 1
                        continue
                    del received[:length]
                    if sid not in ids or sid in out or len(frame[5:-1]) != 31:
                        rejected_frames.append(dict(id=sid, reason='id/length/duplicate', hex=frame.hex()))
                        continue
                    now = time.monotonic()
                    out[sid] = dict(**decode(frame[5:-1], frame[4]),
                                    received=now, readMs=(now-started)*1000)
        finally:
            s.timeout = timeout
        self.last_read = dict(kind='sync-read', mono=started, requestedIds=list(ids),
            receivedIds=list(out), missingIds=[sid for sid in ids if sid not in out],
            elapsedMs=round((time.monotonic()-started)*1000, 3),
            discardedBeforeRead=before_clear, rxBytes=len(raw), checksumErrors=checksum_errors,
            discardedBytes=discarded_bytes, pendingHex=received.hex(), rejectedFrames=rejected_frames,
            txHex=packet.hex(), rxHex=raw.hex(),
            replyMs={sid:round(row['readMs'],3) for sid,row in out.items()})
        if hasattr(self, 'trace'): self.trace.append(self.last_read)
        return out


class ServoPoller:
    """Independent read-only worker; no web/IMU objects cross process boundary."""
    def __init__(self, port, ids, events, stop, period=.02, commands=None, results=None, off=None, off_results=None, statuses=None, cancelled_before=None, sensors=None, policy_halt=None):
        self.port, self.ids, self.events, self.stop, self.period = port, ids, events, stop, period
        self.commands, self.results = commands, results
        self.off, self.off_results, self.statuses = off, off_results, statuses
        self.cancelled_before = cancelled_before
        self.sensors,self.policy_halt=sensors,policy_halt

    def publish(self, rows, error='', scan_ms=None, diagnostics=None):
        now = time.monotonic()
        rows = [dict(row) for row in rows]
        for row in rows:
            if 'received' in row:
                row['ageMs'] = max(0, (now-row.pop('received'))*1000)
            else:
                row.setdefault('ageMs', 0)
        event = (dict(configuredIds=list(self.ids), servos=rows, error=error,
                      scanMs=scan_ms, targetHz=1/self.period, readMode='sync-read',
                      diagnostics=diagnostics), time.monotonic())
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

    def control_status(self, value):
        if self.statuses is not None:
            try: self.statuses.put_nowait(value)
            except queue.Full: pass

    def execute_off(self, bus):
        if self.off is None or not self.off.is_set(): return
        try:
            if bus is None: raise ValueError('串口不可用，全部失能未确认')
            from servo_control import torque_off
            result = torque_off(bus)
        except Exception as exc:
            result = dict(state='failed', message=str(exc))
        self.off.clear()
        self.off_results.put(result)

    def execute_command(self, bus):
        if self.commands is None: return
        try: command = self.commands.get_nowait()
        except queue.Empty: return
        try:
            if bus is None: raise ValueError('串口不可用，未执行硬件标定')
            if self.off is not None and self.off.is_set(): raise ValueError('全部失能已请求，取消排队任务')
            if isinstance(command, dict):
                if command['issued'] <= self.cancelled_before.value: raise ValueError('任务已被全部失能取消')
                from servo_control import execute_control
                cancelled=lambda: self.stop.is_set() or (self.off is not None and self.off.is_set())
                if command['action']=='policy':
                    from policy_control import execute_policy
                    result=execute_policy(bus,command,cancelled,self.publish,self.control_status,self.sensors,self.policy_halt)
                else:
                    result = execute_control(bus, command,cancelled,self.publish,self.control_status)
            else:
                plan, path = command
                from pathlib import Path
                from pose_calibration import calibrate_hardware
                result = calibrate_hardware(bus, plan, Path(path))
            self.results.put((True, result))
        except Exception as exc:
            self.results.put((False, str(exc)))

    def run(self):
        while not self.stop.is_set():
            bus = None
            try:
                bus = ReadOnlyBus(self.port)
                scans = deque(maxlen=100)
                while not self.stop.is_set():
                    self.execute_command(bus)
                    self.execute_off(bus)
                    start = time.monotonic()
                    # A transient missing reply must not suppress the ID for a
                    # full second. Sync-read has one shared deadline for all IDs.
                    feedback = bus.read_feedback_many(self.ids)
                    rows = []
                    for sid in self.ids:
                        if sid in feedback:
                            rows.append(dict(id=sid, online=True, **feedback[sid]))
                        else:
                            rows.append(dict(id=sid, online=False))
                    now = time.monotonic()
                    for row in rows:
                        row['ageMs'] = max(0, (now-row.pop('received', now))*1000)
                    scans.append(start)
                    read = getattr(bus, 'last_read', None) or {}
                    diagnostics = {key: read.get(key) for key in (
                        'missingIds', 'elapsedMs', 'rxBytes', 'checksumErrors',
                        'discardedBytes', 'discardedBeforeRead')}
                    diagnostics['observedScanHz'] = ((len(scans)-1)/(scans[-1]-scans[0])
                        if len(scans)>1 and scans[-1]>scans[0] else None)
                    self.publish(rows, scan_ms=(now-start)*1000, diagnostics=diagnostics)
                    self.stop.wait(max(0, self.period-(time.monotonic()-start)))
            except Exception as exc:
                self.execute_command(None)
                self.execute_off(None)
                self.publish([dict(id=sid, online=False) for sid in self.ids],
                             'Serial unavailable: ' + type(exc).__name__)
                self.stop.wait(2)
            finally:
                if bus:
                    bus.close()


def run_servo_poller(port, ids, events, stop, commands, results, off, off_results, statuses, cancelled_before, sensors, policy_halt):
    # Exit never waits for a final unread telemetry packet to flush.
    events.cancel_join_thread()
    statuses.cancel_join_thread()
    ServoPoller(port, ids, events, stop, commands=commands, results=results, off=off, off_results=off_results, statuses=statuses, cancelled_before=cancelled_before,sensors=sensors,policy_halt=policy_halt).run()


class ServoSource:
    def __init__(self, store, port, ids):
        self.store = store
        context = multiprocessing.get_context('spawn')
        self.stop = context.Event()
        self.events = context.Queue(maxsize=1)
        self.commands = context.Queue(maxsize=1)
        self.results = context.Queue(maxsize=1)
        self.pending = None
        self.off = context.Event()
        self.cancelled_before = context.Value("d", 0.)
        self.off_results = context.Queue(maxsize=1)
        self.statuses = context.Queue(maxsize=32)
        self.sensors=ImuMailbox(context)
        self.policy_halt=context.Event()
        self.off_pending = None
        self.pending_kind = 'calibration'
        self.control = dict(state='idle', message='等待操作')
        # Keep start/join compatibility with the existing lifespan owner.
        self.thread = context.Process(target=run_servo_poller,
            args=(port, ids, self.events, self.stop, self.commands, self.results, self.off, self.off_results, self.statuses, self.cancelled_before,self.sensors,self.policy_halt), daemon=True, name='servo-observer')

    def update_sensors(self, samples):
        self.sensors.update(samples)

    def submit_calibration(self, plan, path):
        if self.pending is not None or self.off_pending is not None: raise ValueError('舵机任务仍在执行')
        if not self.thread.is_alive(): raise ValueError('串口采集进程未运行')
        future = concurrent.futures.Future()
        # Queued work cannot be cancelled once the child may see it.
        future.set_running_or_notify_cancel()
        self.commands.put_nowait((plan, str(path)))
        self.pending_kind = "calibration"
        self.pending_rows = plan["rows"]
        self.pending = future
        return future

    def submit_control(self, command):
        if self.pending is not None or self.off_pending is not None: raise ValueError('舵机任务仍在执行')
        if not self.thread.is_alive(): raise ValueError('串口采集进程未运行')
        future = concurrent.futures.Future(); future.set_running_or_notify_cancel()
        if command['action']=='policy': self.policy_halt.clear()
        self.commands.put_nowait({**command, "issued": time.monotonic()})
        self.pending, self.pending_kind = future, 'control'
        self.control = dict(state='preflight', action=command['action'], message='检查中', progress=0)
        return future

    def submit_off(self):
        if self.off_pending is not None: return self.off_pending
        if not self.thread.is_alive(): raise ValueError('串口采集进程未运行，失能未确认')
        future = concurrent.futures.Future(); future.set_running_or_notify_cancel()
        self.off_pending = future
        self.cancelled_before.value = time.monotonic()
        self.off.set()
        self.control = dict(state='disabling', message='正在全部失能')
        return future

    def drain_results(self):
        while True:
            try: status = self.statuses.get_nowait()
            except queue.Empty: break
            if self.pending_kind == 'control' and self.pending is not None and self.off_pending is None:
                self.control = status
        if self.off_pending is not None:
            try: off_result = self.off_results.get_nowait()
            except queue.Empty:
                off_result = None if self.thread.is_alive() else dict(state='failed', message='串口进程退出，失能未确认')
            if off_result is not None:
                self.control = off_result
                future, self.off_pending = self.off_pending, None
                future.set_result(off_result)
        if self.pending is None: return
        try: ok, result = self.results.get_nowait()
        except queue.Empty:
            if self.thread.is_alive(): return
            ok, result = False, '串口进程退出，任务结果未知；请检查实物状态及备份'
        future, self.pending = self.pending, None
        if self.pending_kind == 'control' and self.control['state'] not in ('disabling', 'disabled'):
            self.control = result if ok else dict(state='failed', message=str(result))
        if ok: future.set_result(result)
        else: future.set_exception(ValueError(result))

    def close(self):
        self.stop.set()
        self.thread.join(2)
        if self.thread.is_alive():
            self.thread.terminate()
            self.thread.join(2)
        self.drain_results()
        self.events.close()
        self.commands.close()
        self.results.close()
        self.off_results.close()
        self.statuses.close()

    def drain(self):
        self.drain_results()
        try:
            data, timestamp = self.events.get_nowait()
        except queue.Empty:
            return
        item = self.store.sample('joints', data, timestamp=timestamp)
        item['source'] = 'hardware'
