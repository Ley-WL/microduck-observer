import asyncio
import json
import time
import urllib.request
from pathlib import Path
import websockets
from datetime import datetime

SESSION = datetime.now().strftime('%H%M%S')

async def main():
    output = Path(__file__).with_name('telemetry-' + SESSION + '.jsonl')
    with output.open('a', encoding='utf-8') as log:
        last_health = 0.
        def save(data):
            nonlocal last_health
            log.write(json.dumps({'wallTime': time.time(), **data}, ensure_ascii=False) + '\n')
            log.flush()
            if time.monotonic() - last_health >= 1:
                temp = Path(__file__).with_name('health.tmp')
                temp.write_text(json.dumps({'wallTime': time.time(), 'file': output.name, 'pid': __import__('os').getpid(), 'lastType': data.get('message', {}).get('topic', data.get('captureType'))}), encoding='utf-8')
                temp.replace(Path(__file__).with_name('health.json'))
                last_health = time.monotonic()
        async def controls():
            def read():
                start = time.time()
                with urllib.request.urlopen('http://127.0.0.1:18877/api/v1/policy', timeout=3) as response:
                    value = json.load(response)
                return {'captureType': 'control', 'requestStart': start, 'requestMs': (time.time()-start)*1000, 'control': value.get('control')}
            while True:
                begin = time.monotonic()
                try:
                    save(await asyncio.to_thread(read))
                except Exception as exc:
                    save({'captureType': 'control-error', 'error': str(exc)})
                await asyncio.sleep(max(0.1, .25-(time.monotonic()-begin)))
        with urllib.request.urlopen('http://127.0.0.1:18877/api/v1/snapshot', timeout=5) as response:
            save({'captureType': 'baseline', 'readOnly': True, 'snapshot': json.load(response)})
        async with websockets.connect('ws://127.0.0.1:18877/api/v1/stream', origin='http://127.0.0.1:18877') as ws:
            await ws.send(json.dumps({'type': 'subscribe', 'requestId': 'tilt-readonly', 'topics': {'imu.orientation': 50, 'imu.raw': 50, 'joints': 50, 'logs': None, 'system': 1}}))
            task = asyncio.create_task(controls())
            try:
                end = time.monotonic() + 1800
                while time.monotonic() < end:
                    message = json.loads(await asyncio.wait_for(ws.recv(), 5))
                    save({'message': message})
            finally:
                task.cancel()
                try:
                    await task
                except asyncio.CancelledError:
                    pass

async def resilient():
    while True:
        try:
            await main()
            return
        except Exception as exc:
            with Path(__file__).with_name('connection-events.jsonl').open('a', encoding='utf-8') as log:
                log.write(json.dumps({'wallTime': time.time(), 'error': str(exc)}) + '\n')
            await asyncio.sleep(3)

asyncio.run(resilient())
