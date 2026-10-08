from pathlib import Path
import collections,json,re
import sys

root=Path(__file__).resolve().parents[1]
directory=root/'docs/实测记录/附件/HAT/2026-10-04/servo-angle-feedback'
prefix='native-servo-loop-all14' if '--all14' in sys.argv else 'native-servo-loop'
if '--io-cycle' in sys.argv:
    prefix+='-io-cycle'
    directory=root/'docs/实测记录/附件/HAT/2026-10-04/servo-io-cycle'
report=json.loads((directory/(prefix+'-acceptance.json')).read_text(encoding='utf-8'))
manifest=json.loads((directory/(prefix+'-manifest.json')).read_text(encoding='utf-8'))
frames=report.get('frames',[])
def summarize(items):
    missing=collections.Counter();positions=[];consecutive=maximum=0
    for frame in items:
        diagnostic=frame['diagnostics'];missing.update(diagnostic['missingIds'])
        consecutive=consecutive+1 if diagnostic['missingIds'] else 0;maximum=max(maximum,consecutive)
        if '23' in frame['feedback']:positions.append(frame['feedback']['23']['position'])
    times=[frame['writeMono'] for frame in items]
    return {'commands':len(items),'actualCommandHz':(len(times)-1)/(times[-1]-times[0]) if len(times)>1 else None,
            'maxCommandGapMs':max((b-a)*1000 for a,b in zip(times,times[1:])) if len(times)>1 else None,
            'expectedResponses':len(items)*15,'missingResponses':sum(missing.values()),'missingById':dict(missing),
            'missingResponsePercent':sum(missing.values())/(len(items)*15)*100 if items else None,
            'incompleteFrames':sum(bool(frame['diagnostics']['missingIds']) for frame in items),
            'checksumErrors':sum(frame['diagnostics']['checksumErrors'] for frame in items),
            'checksumErrorFrames':sum(frame['diagnostics']['checksumErrors']>0 for frame in items),
            'maxConsecutiveIncomplete':maximum,'coastedFrames':sum(frame['coasted'] for frame in items),
            'holdingFrames':sum(frame['holding'] for frame in items),
            'readLengths':dict(collections.Counter(frame['diagnostics']['readLength'] for frame in items)),
            'position23Range': [min(positions),max(positions)] if positions else None}
def kernel(text):
    line=next(line for line in text.splitlines() if line.startswith('2:'))
    return {key:int(value) for key,value in re.findall(r'\b(tx|rx|fe|oe|brk|pe):(\d+)',line)}
before=kernel(manifest['kernelBefore']);after=kernel(manifest['kernelAfter'])
summary={'total':summarize(frames),'rounds':{str(n):summarize([f for f in frames if f['round']==n]) for n in range(1,4)},
         'skippedTicks':report.get('skippedTicks'),'kernelCounterDeltas':{key:after.get(key,0)-before.get(key,0) for key in set(before)|set(after)},
         'calibrationUnchanged':manifest['calibrationShaBefore']==manifest['calibrationShaAfter'],
         'finalDisabled':report.get('disable'),'error':report.get('error'),
         'scope':'Native supported-joint movement/read-all15; production I/O/clock/coast reused; no IMU/model inference'}
summary['positionRangesById']={}
for id in report.get('controlledIds',[23]):
    positions=[f['feedback'][str(id)]['position'] for f in frames if str(id) in f['feedback']]
    if positions:summary['positionRangesById'][str(id)]={'min':min(positions),'max':max(positions),'spanDeg':(max(positions)-min(positions))*360/4096}
if report.get('centers'):
    summary['restoreErrorDegById']={id:abs(report['restore'][id]['position']-center)*360/4096 for id,center in report['centers'].items() if id in report.get('restore',{})}
invariants=['goalCurrentRaw','kpRaw','kiRaw','kdRaw']
summary['currentAndPidUnchanged']=all(report['baseline'][id][key]==report['final'][id][key] for id in report.get('final',{}) for key in invariants)
(directory/(prefix+'-analysis.json')).write_text(json.dumps(summary,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(summary,ensure_ascii=False))
