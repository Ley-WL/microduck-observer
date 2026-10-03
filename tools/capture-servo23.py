"""Read-only WS capture for user-operated fine adjustments."""
import asyncio, json, time
from pathlib import Path
import websockets

async def main():
    out = Path(__file__).resolve().parents[1] / 'docs/实测记录/附件/平台/2026-10-02/servo23-fine-feedback.json'
    rows = []
    started = time.time()
    async with websockets.connect('ws://192.168.31.193:8877/api/v1/stream', origin='http://192.168.31.193:8877') as ws:
        await ws.send(json.dumps({'type': 'subscribe', 'topics': {'joints': 50, 'logs': None, 'system': 5}}))
        until = time.monotonic() + 60
        while time.monotonic() < until:
            try:
                msg = json.loads(await asyncio.wait_for(ws.recv(), 2))
            except asyncio.TimeoutError:
                continue
            if msg.get('topic') == 'joints':
                if not rows:
                    print('Live joint feedback connected', flush=True)
                row = next((r for r in msg['data']['servos'] if r['id'] == 23), None)
                rows.append({'received': time.time(), 'mono': msg.get('sampleMonoMs'), 'servo': row})
            elif msg.get('type') == 'log_batch':
                for item in msg['items']:
                    if '#23' in item['data'].get('message', ''):
                        rows.append({'received': time.time(), 'log': item})
            elif msg.get('topic') == 'system':
                rows.append({'received': time.time(), 'control': msg['data'].get('servoControl')})
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({'started': started, 'readOnly': True, 'rows': rows}, ensure_ascii=False), encoding='utf-8')
    print(f'Saved {len(rows)} records: {out}')

asyncio.run(main())
