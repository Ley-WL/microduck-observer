"""Only subscribe to production telemetry and read kernel counters. No UART opens."""
import asyncio,json,time,collections,statistics,subprocess
from contextlib import AsyncExitStack
from pathlib import Path
try:
    import websockets
except ImportError:
    import ws_reader as websockets

out=Path('/home/radxa/servo-r33-audit-20261013')
out.mkdir(exist_ok=True)
raw=out/'after-r33-frames-v3.jsonl'
assert not raw.exists(),'Preserve existing capture; do not repeat it'

def counters():return Path('/proc/tty/driver/serial').read_text()
async def main():
    before=counters();start=time.monotonic();counts=collections.Counter();missing=collections.Counter()
    (out/'after-r33-start-v3.json').write_text(json.dumps(dict(
        serialBefore=before,bootId=Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
        uptime=Path('/proc/uptime').read_text(),hardwareMotion=False,uartOpened=False),indent=2))
    read_times=[];voltages=[];zero=0;complete=0;bad=0;received=0;seqs=set();states=collections.Counter()
    async with AsyncExitStack() as stack:
        ws=await stack.enter_async_context(websockets.connect('ws://127.0.0.1:8877/api/v1/stream'))
        f=stack.enter_context(raw.open('w'))
        await ws.send(json.dumps({'type':'subscribe','topics':{'joints':100,'system':1}}))
        while time.monotonic()-start<30:
            sample=json.loads(await asyncio.wait_for(ws.recv(),4))
            if sample.get('type')!='sample':continue
            f.write(json.dumps(sample,separators=(',',':'))+'\n')
            f.flush()
            if sample['topic']=='system':states[sample['data'].get('servoControl',{}).get('state','unknown')]+=1;continue
            if sample['topic']!='joints':continue
            if sample['seq'] in seqs:continue
            seqs.add(sample['seq']);data=sample['data'];diag=data.get('diagnostics',{})
            offline=[r['id'] for r in data['servos'] if not r['online']]
            missing.update(offline);complete+=not offline;zero+=diag.get('rxBytes')==0
            bad+=diag.get('checksumErrors',0);received+=diag.get('rxBytes',0)
            counts[len(data['servos'])-len(offline)]+=1
            read_times.append(diag.get('elapsedMs',0))
            voltages.extend(r['voltage'] for r in data['servos'] if r['online'] and r.get('voltage') is not None)
    result=dict(seconds=time.monotonic()-start,uniqueJointSamples=len(seqs),completeSamples=complete,zeroRxSamples=zero,
        onlineCountHistogram=dict(counts),missingById=dict(missing),checksumErrorsInSampledReads=bad,
        sampledRxBytes=received,readMsMean=statistics.mean(read_times),readMsMax=max(read_times),
        freshVoltageMin=min(voltages) if voltages else None,freshVoltageMax=max(voltages) if voltages else None,
        controlStates=dict(states),serialBefore=before,serialAfter=counters(),hardwareMotion=False,uartOpened=False)
    (out/'after-r33-summary.json').write_text(json.dumps(result,indent=2))
    print(json.dumps(result),flush=True)

asyncio.run(main())
