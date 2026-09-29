"""Read-only bridge for tofd's newline-delimited JSON-RPC stream."""
import asyncio
import json
import time
from collections import deque


def validate_frame(frame):
    if not isinstance(frame, dict) or frame.get('rows') != 8 or frame.get('cols') != 8:
        raise ValueError('Expected an 8x8 ToF frame')
    for key in ('distance_mm', 'status'):
        values = frame.get(key)
        if not isinstance(values, list) or len(values) != 64 or any(
            type(v) is not int or not 0 <= v <= (65535 if key == 'distance_mm' else 255) for v in values
        ):
            raise ValueError('Invalid ToF ' + key)
    if any(type(frame.get(k)) is not int or frame[k] < 0 for k in ('seq', 't_ns')):
        raise ValueError('Invalid ToF sequence/timestamp')
    return frame


class TofSource:
    def __init__(self, sink, path):
        self.sink, self.path = sink, path
        self.state, self.error = 'connecting', ''
        self.last_received = None
        self.hz = 0.0

    def health(self):
        age = None if self.last_received is None else (time.monotonic() - self.last_received) * 1000
        return dict(state='stale' if self.state == 'streaming' and age > 1500 else self.state,
                    error=self.error, ageMs=age, hz=self.hz)

    async def run(self):
        while True:
            writer = None
            try:
                reader, writer = await asyncio.wait_for(asyncio.open_unix_connection(self.path, limit=65536), 3)
                writer.write(b'{"jsonrpc":"2.0","id":1,"method":"tof.stream","params":{}}\n')
                await writer.drain()
                timestamps = deque(maxlen=30)
                previous = None
                while True:
                    line = await asyncio.wait_for(reader.readline(), 3)
                    if not line:
                        raise ConnectionError('ToF stream closed')
                    message = json.loads(line)
                    if message.get('error'):
                        raise ValueError('ToF subscription rejected')
                    if message.get('method') != 'tof.frame':
                        continue
                    frame = validate_frame(message.get('params'))
                    if previous is not None and (frame['seq'] <= previous['seq'] or frame['t_ns'] <= previous['t_ns']):
                        raise ValueError('ToF sequence/timestamp reset')
                    timestamps.append(frame['t_ns'])
                    self.hz = (len(timestamps)-1)*1e9/(timestamps[-1]-timestamps[0]) if len(timestamps)>1 else 0
                    self.last_received = time.monotonic()
                    if self.state != 'streaming':
                        self.sink.log('INFO', 'tof', 'ToF 硬件流已连接 · 8×8')
                    self.state, self.error = 'streaming', ''
                    item = self.sink.sample('tof', dict(rows=8, cols=8, distanceMm=frame['distance_mm'],
                        status=frame['status'], sensorSeq=frame['seq'], sensorTimeNs=frame['t_ns'],
                        hz=self.hz, sequenceGap=0 if previous is None else frame['seq']-previous['seq']-1,
                        sensor='VL53L5CX'), timestamp=self.last_received)
                    item['source'] = 'hardware'
                    previous = frame
            except (OSError, ValueError, TypeError, AttributeError, asyncio.TimeoutError) as exc:
                error = str(exc) or 'ToF stream timeout'
                if self.state != 'offline' or self.error != error:
                    self.sink.log('WARN', 'tof', error)
                self.state, self.error, self.hz = 'offline', error, 0.0
            finally:
                if writer:
                    writer.close()
                    try:
                        await writer.wait_closed()
                    except OSError:
                        pass
            await asyncio.sleep(2)
