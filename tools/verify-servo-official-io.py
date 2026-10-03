"""Three read-only WS observation windows for the deployed HD1910 I/O optimization."""
import asyncio,json,time,urllib.request,subprocess,re
from collections import Counter
from pathlib import Path
import websockets

root=Path(__file__).resolve().parents[1]
base=root/'docs/实测记录/附件/HAT/2026-10-03'
password=next(x.removeprefix('Password: ') for x in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if x.startswith('Password: '))
def kernel():
    r=subprocess.run(['ssh','-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193','radxa@192.168.31.186',"sudo -S -p '' cat /proc/tty/driver/serial"],input=password+'\n',text=True,capture_output=True)
    r.check_returncode();return r.stdout
def counts(text):
    line=next(x for x in text.splitlines() if x.startswith('2:'))
    return {k:int(v) for k,v in re.findall(r'(fe|oe):(\d+)',line)}
def snapshot():return json.load(urllib.request.urlopen('http://192.168.31.186:8877/api/v1/snapshot',timeout=5))
async def main():
    summary=[]
    for repetition in range(1,4):
        out={'round':repetition,'kernelBefore':kernel(),'before':snapshot(),'samples':[],'hardwareCommands':0}
        expected_torque={r['id']:r['torque'] for r in out['before']['joints']['data']['servos'] if r['online']}
        start=time.monotonic()
        async with websockets.connect('ws://192.168.31.186:8877/api/v1/stream') as ws:
            await ws.send(json.dumps({'type':'subscribe','topics':{'joints':50}}))
            while len(out['samples'])<1000 and time.monotonic()-start<30:
                sample=json.loads(await asyncio.wait_for(ws.recv(),5))
                if sample.get('type')=='sample' and sample.get('topic')=='joints':
                    out['samples'].append(sample)
                    assert all(r['torque']==expected_torque.get(r['id']) and r['fault']==0 for r in sample['data']['servos'] if r['online'])
        out['elapsedSeconds']=time.monotonic()-start;out['after']=snapshot();out['kernelAfter']=kernel()
        modes={};missing=Counter();bad=0
        for sample in out['samples']:
            diag=sample['data']['diagnostics'];length=diag['readLength']
            m=modes.setdefault(length,{'frames':0,'complete':0,'rxBytes':0,'elapsedMs':[]})
            m['frames']+=1;m['rxBytes']+=diag['rxBytes'];m['elapsedMs'].append(diag['elapsedMs'])
            m['complete']+=not diag['missingIds'];missing.update(diag['missingIds']);bad+=diag['checksumErrors']>0
            if diag['checksumErrors']:
                raw=bytes.fromhex(diag['rxHex']);valid=set()
                for i in range(len(raw)-length-5):
                    frame=raw[i:i+length+6]
                    if frame[:2]==b'\xff\xff' and frame[3]==length+2 and sum(frame[2:])%256==255:
                        valid.add(frame[2])
                assert valid==set(diag['receivedIds']),'完整原始包被解析误丢'
        for value in modes.values():
            times=value.pop('elapsedMs');value['meanReadMs']=sum(times)/len(times);value['maxReadMs']=max(times)
        before,after=counts(out['kernelBefore']),counts(out['kernelAfter'])
        report={'round':repetition,'samples':len(out['samples']),'uniqueSeq':len(set(s['seq'] for s in out['samples'])),
                'complete':sum(m['complete'] for m in modes.values()),'checksumFrames':bad,'missingById':dict(missing),'modes':modes,
                'kernelDelta':{k:after.get(k,0)-before.get(k,0) for k in ('fe','oe')},
                'receivedHz':len(out['samples'])/out['elapsedSeconds'],'finalOnline':sum(r['online'] for r in out['after']['joints']['data']['servos']),
                'finalTorque0':sum(r['torque']==0 for r in out['after']['joints']['data']['servos']),
                'torqueBefore':expected_torque,'torqueAfter':{r['id']:r['torque'] for r in out['after']['joints']['data']['servos'] if r['online']}}
        out['report']=report
        (base/f'servo-official-io-verification-{repetition}.json').write_text(json.dumps(out),encoding='utf-8')
        summary.append(report);(base/'servo-official-io-verification-summary.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
        print(json.dumps(report),flush=True)
asyncio.run(main())
