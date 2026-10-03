import asyncio
from collections import Counter
import json
from pathlib import Path
import time
import websockets

async def main():
    frames=[]; missing=Counter(); start=time.monotonic()
    async with websockets.connect('ws://192.168.31.186:8877/api/v1/stream') as ws:
        await ws.send(json.dumps({'type':'subscribe','requestId':'flapping-readonly','topics':{'joints':20,'system':1}}))
        while time.monotonic()-start < 20:
            msg=json.loads(await asyncio.wait_for(ws.recv(),4))
            if msg.get('topic')=='joints':
                frames.append(msg)
                missing.update(r['id'] for r in msg['data']['servos'] if not r['online'])
    result={'seconds':time.monotonic()-start,'frames':len(frames),'complete':sum(all(r['online'] for r in f['data']['servos']) for f in frames),'missing':dict(missing),'checksumErrors':sum(f['data']['diagnostics']['checksumErrors'] for f in frames),'bootIds':list(set(f['bootId'] for f in frames)),'samples':frames}
    (Path(__file__).resolve().parents[1]/'docs/实测记录/附件/平台/2026-10-03/online-flapping-ws.json').write_text(json.dumps(result),encoding='utf-8')
    print(json.dumps({k:v for k,v in result.items() if k!='samples'}))

asyncio.run(main())
