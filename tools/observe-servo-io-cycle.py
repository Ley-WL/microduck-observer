"""Read-only production snapshots after the I/O optimization; no action requests."""
import collections,json,time,urllib.request
from pathlib import Path

root=Path(__file__).resolve().parents[1]
directory=root/'docs/实测记录/附件/HAT/2026-10-04/servo-io-cycle'
directory.mkdir(parents=True,exist_ok=True)
samples=[];before=None;last=None;began=time.monotonic()
while len(samples)<300 and time.monotonic()-began<20:
    snapshot=json.load(urllib.request.urlopen('http://192.168.31.186:8877/api/v1/snapshot',timeout=4))
    if before is None:before=snapshot
    row=snapshot['joints']
    if row['seq']!=last:samples.append(row);last=row['seq']
    time.sleep(.02)
missing=collections.Counter(id for sample in samples for id in sample['data']['diagnostics']['missingIds'])
summary={'snapshots':len(samples),'incomplete':sum(bool(sample['data']['diagnostics']['missingIds']) for sample in samples),
         'missingById':dict(missing),'checksumErrors':sum(sample['data']['diagnostics']['checksumErrors'] for sample in samples),
         'readLengths':dict(collections.Counter(sample['data']['diagnostics']['readLength'] for sample in samples)),
         'observedScanHzLast':samples[-1]['data']['diagnostics'].get('observedScanHz'),
         'enabledBefore':[r['id'] for r in before['joints']['data']['servos'] if r.get('torque')==1],
         'enabledAfter':[r['id'] for r in snapshot['joints']['data']['servos'] if r.get('torque')==1],
         'source':'read-only sampled platform snapshots; not every serial transaction'}
(directory/'readonly-observation.json').write_text(json.dumps({'summary':summary,'before':before,'after':snapshot,'samples':samples}),encoding='utf-8')
print(json.dumps(summary))
