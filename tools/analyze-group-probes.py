from pathlib import Path
import json, re

base = Path(__file__).resolve().parents[1] / 'docs/实测记录/附件/HAT/2026-10-03'
results=[]
for path in sorted(base.glob('group-probe-*.json')):
    if not re.search(r'-\d+\.json$', path.name): continue
    data=json.loads(path.read_text())
    samples=data['samples']; last=samples[-1]['diagnostics'] if samples else {}
    raw=bytes.fromhex(last.get('rxHex',''))
    valid=set()
    for offset in range(len(raw)-5):
        if raw[offset:offset+2]!=b'\xff\xff': continue
        length=raw[offset+3]+4; frame=raw[offset:offset+length]
        if length==37 and len(frame)==37 and sum(frame[2:])%256==255:
            valid.add(frame[2])
    def uart(text):
        line=next(x for x in text.splitlines() if x.startswith('2:'))
        return {k:int(v) for k,v in re.findall(r'(fe|oe):(\d+)',line)}
    before,after=uart(data['kernelBefore']),uart(data['kernelAfter'])
    group,roundno=path.stem.removeprefix('group-probe-').rsplit('-',1)
    motion={}
    commanded_ids=set().union(*(set(x['targets']) for x in data['commands']))
    for sid in sorted(commanded_ids,key=int):
        center=data['baseline'][str(sid)]['position']
        values=[(s['feedback'][str(sid)]['position']-center+2048)%4096-2048
                for s in samples if str(sid) in s['feedback']]
        if values:
            motion[str(sid)]={'minOffsetDeg':min(values)*360/4096,
                              'maxOffsetDeg':max(values)*360/4096,
                              'excursionDeg':(max(values)-min(values))*360/4096}
    results.append({'group':group,'round':int(roundno),'result':data['result'],
        'readFrames':len(samples),'positionCommands':sum(bool(x['targets']) for x in data['commands']),
        'missingIds':last.get('missingIds',[]),'rxBytes':last.get('rxBytes'),
        'checksumErrors':last.get('checksumErrors',0),'discardedBytes':last.get('discardedBytes',0),
        'rawValidIds':sorted(valid),'rawMissingIds':sorted(set(last.get('requestedIds',[]))-valid),
        'kernelDelta':{k:after.get(k,0)-before.get(k,0) for k in ('fe','oe')},
        'motionFeedback':motion,
        'observedHz':data['timing']['observedHz'],'maxGapMs':data['timing']['maxGapMs'],
        'unload':data.get('unload')})
(base/'group-probe-analysis.json').write_text(json.dumps(results,indent=2),encoding='utf-8')
for r in results:
    print(json.dumps(r,ensure_ascii=False))
