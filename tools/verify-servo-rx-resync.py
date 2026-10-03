"""Sample the deployed service without issuing any actuator command."""
import asyncio,json,time,urllib.request,subprocess
from pathlib import Path
import websockets

root=Path(__file__).resolve().parents[1]
base=root/'docs/实测记录/附件/HAT/2026-10-03'
password=next(x.removeprefix('Password: ') for x in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if x.startswith('Password: '))
def kernel():
    r=subprocess.run(['ssh','-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193','radxa@192.168.31.186',"sudo -S -p '' cat /proc/tty/driver/serial"],input=password+'\n',text=True,capture_output=True)
    r.check_returncode();return r.stdout
def snapshot():return json.load(urllib.request.urlopen('http://192.168.31.186:8877/api/v1/snapshot',timeout=5))
async def main():
    out={'kernelBefore':kernel(),'before':snapshot(),'samples':[],'hardwareCommands':0}
    start=time.monotonic()
    async with websockets.connect('ws://192.168.31.186:8877/api/v1/stream') as ws:
        await ws.send(json.dumps({'type':'subscribe','topics':{'joints':50}}))
        while len(out['samples'])<1000 and time.monotonic()-start<30:
            sample=json.loads(await asyncio.wait_for(ws.recv(),5))
            if sample.get('type')=='sample' and sample.get('topic')=='joints':
                out['samples'].append(sample)
                assert all(r['torque']==0 for r in sample['data']['servos'] if r['online'])
    out['elapsedSeconds']=time.monotonic()-start
    out['after']=snapshot();out['kernelAfter']=kernel()
    report={'sampledFrames':len(out['samples']),'completeFrames':sum(not s['data']['diagnostics']['missingIds'] for s in out['samples']),
            'checksumErrorFrames':sum(s['data']['diagnostics']['checksumErrors']>0 for s in out['samples']),
            'finalOnline':sum(r['online'] for r in out['after']['joints']['data']['servos']),
            'finalTorque0':sum(r['torque']==0 for r in out['after']['joints']['data']['servos'])}
    out['report']=report
    (base/'servo-rx-resync-verification.json').write_text(json.dumps(out),encoding='utf-8')
    print(json.dumps(report))
asyncio.run(main())
