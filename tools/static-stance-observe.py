"""Supported static stance: read-only snapshots, target constancy and joint tracking."""
import json,time,urllib.request,collections
import sys
from pathlib import Path

root=Path(__file__).resolve().parents[1]
directory=root/'docs/实测记录/附件/HAT/2026-10-04/static-stance-supported'
directory.mkdir(parents=True,exist_ok=True)
samples=[];last=None;start=time.monotonic()
while time.monotonic()-start<(5 if '--after-reference' in sys.argv else 15):
    snapshot=json.load(urllib.request.urlopen('http://192.168.31.186:8877/api/v1/snapshot',timeout=4))
    if snapshot['joints']['seq']!=last:
        samples.append(snapshot);last=snapshot['joints']['seq']
    time.sleep(.025)
summary={'samples':len(samples),'elapsedSeconds':time.monotonic()-start,'joints':{},
         'controlStates':dict(collections.Counter(s['system']['data']['servoControl']['state'] for s in samples)),
         'calibrationRevisions':sorted({s['calibration']['revision'] for s in samples}),
         'missingFrames':sum(bool(s['joints']['data']['diagnostics']['missingIds']) for s in samples),
         'checksumErrors':sum(s['joints']['data']['diagnostics']['checksumErrors'] for s in samples)}
for id in [10,11,12,13,14,20,21,22,23,24,30,31,32,33,34]:
    rows=[r for s in samples for r in s['joints']['data']['servos'] if r['id']==id and r['online']]
    positions=[r['position'] for r in rows]
    errors=[abs(r['position']-r['goalPositionRaw'])*360/4096 for r in rows]
    summary['joints'][str(id)]={'positionRange':[min(positions),max(positions)] if positions else None,
                              'spanDeg':(max(positions)-min(positions))*360/4096 if positions else None,
                              'maxGoalErrorDeg':max(errors) if errors else None,
                              'goalValues':sorted({r['goalPositionRaw'] for r in rows}),
                              'internalTargetValues':sorted({r['target'] for r in rows}),
                              'torqueValues':sorted({r['torque'] for r in rows}),
                              'faultValues':sorted({r['fault'] for r in rows}),
                              'maxAbsCurrentRaw':max(abs(r['currentRaw']) for r in rows) if rows else None}
(directory/('observation-after-reference.json' if '--after-reference' in sys.argv else 'observation.json')).write_text(json.dumps({'summary':summary,'samples':samples}),encoding='utf-8')
print(json.dumps(summary))
