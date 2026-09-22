"""Verify the deployed service through LAN HTTP and WebSocket, without I2C access."""
import argparse
import asyncio
from collections import Counter
import json
import math
import time
from urllib.request import urlopen
import websockets

parser = argparse.ArgumentParser()
parser.add_argument('url')
parser.add_argument('--seconds', type=float, default=12)
args = parser.parse_args()
base = args.url.rstrip('/')


def get(path):
    with urlopen(base + path, timeout=5) as response:
        return json.load(response)


async def check():
    info = get('/api/v1/info')
    assert info['source'] == 'hardware'
    assert not info['capabilities']['pose']
    counts = Counter()
    sequences = {}
    start = time.monotonic()
    before = get('/api/v1/health')
    async with websockets.connect(base.replace('http', 'ws', 1) + '/api/v1/stream', origin=base) as ws:
        await ws.send(json.dumps({'type':'subscribe', 'topics':{'imu.orientation':50,'imu.raw':50,'system':1,'logs':None,'joints':20}}))
        while time.monotonic() - start < args.seconds:
            message = json.loads(await asyncio.wait_for(ws.recv(), 5))
            if message['type'] != 'sample':
                continue
            topic = message['topic']
            assert message['source'] == 'hardware'
            assert message['seq'] > sequences.get(topic, -1)
            assert message['valid']
            assert 0 <= message['ageMs'] < 1500
            if topic == 'joints':
                assert all(r['online'] for r in message['data']['servos'])
            if topic == 'imu.orientation':
                assert message['data']['frame'] == 'sensor'
                assert abs(math.hypot(*message['data']['quaternion'])-1) < .05
            sequences[topic] = message['seq']
            counts[topic] += 1
    elapsed = time.monotonic()-start
    after = get('/api/v1/health')
    assert after['status'] == 'ok'
    assert counts['imu.orientation'] > args.seconds*20
    assert counts['imu.raw'] > args.seconds*20
    assert before['imu']['ioErrors'] == after['imu']['ioErrors']
    assert before['imu']['unparsedReports'] == after['imu']['unparsedReports']
    print(json.dumps({'elapsedSeconds':round(elapsed,3), 'received':dict(counts),
        'receivedHz':{k:round(v/elapsed,2) for k,v in counts.items()}, 'health':after}, ensure_ascii=True))


asyncio.run(check())
