import asyncio
import json
import unittest
from unittest.mock import patch, AsyncMock
from tof import TofSource, validate_frame


def frame(seq=1):
    return dict(rows=8, cols=8, distance_mm=[358]*64, status=[5]*63+[0], seq=seq, t_ns=seq*74000000)


class ValidationTests(unittest.TestCase):
    def test_real_fixture_and_invalid_shapes(self):
        from pathlib import Path
        path = Path(__file__).resolve().parents[1] / 'docs/实测记录/附件/ToF/2026-09-29/接牢后目标复测.json'
        if path.exists():
            for sample in json.loads(path.read_text(encoding='utf-8'))['frames']:
                validate_frame(sample)
        for change in ({'distance_mm': [1]}, {'status': [True]*64}, {'rows': 4}, {'t_ns': -1}):
            with self.assertRaises(ValueError):
                validate_frame({**frame(), **change})


class StreamTests(unittest.IsolatedAsyncioTestCase):
    async def test_bridge_disconnect_and_reconnect(self):
        class Sink:
            def __init__(self): self.items = []
            def log(self, *args): pass
            def sample(self, topic, data, **kwargs):
                item = dict(topic=topic, data=data, **kwargs)
                self.items.append(item)
                return item
        sink = Sink()
        source = TofSource(sink, '/test.sock')
        first, second = asyncio.StreamReader(), asyncio.StreamReader()
        for n in (1, 3):
            first.feed_data((json.dumps(dict(method='tof.frame', params=frame(n)))+'\n').encode())
        first.feed_eof()
        second.feed_data((json.dumps(dict(method='tof.frame', params=frame(1)))+'\n').encode())
        writer = AsyncMock()
        from unittest.mock import Mock
        writer.write, writer.close = Mock(), Mock()
        with patch('tof.asyncio.open_unix_connection', create=True, new=AsyncMock(side_effect=[(first, writer), (second, writer)])):
            task = asyncio.create_task(source.run())
            try:
                async with asyncio.timeout(1):
                    while source.state != 'offline': await asyncio.sleep(.01)
                self.assertEqual(sink.items[-1]['data']['sequenceGap'], 1)
                self.assertEqual(source.health()['state'], 'offline')
                async with asyncio.timeout(3):
                    while len(sink.items) < 3: await asyncio.sleep(.01)
                self.assertEqual(sink.items[-1]['source'], 'hardware')
                self.assertEqual(sink.items[-1]['data']['sequenceGap'], 0)
                self.assertEqual(source.health()['state'], 'streaming')
                source.last_received -= 2
                self.assertEqual(source.health()['state'], 'stale')
            finally:
                task.cancel()
                with self.assertRaises(asyncio.CancelledError): await task
        writer.close.assert_called()
