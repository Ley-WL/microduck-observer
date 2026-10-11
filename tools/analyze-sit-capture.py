"""Compare the captured actor targets with calibrated physical feedback."""
import json, math, statistics
from pathlib import Path
root=Path(__file__).resolve().parents[1]
directory=root/'docs/实测记录/附件/平台/2026-10-10/bidirectional-sit-inspect'
data=json.loads((directory/'timeline.json').read_text(encoding='utf-8'))
cal=json.loads((directory/'calibration-capture.json').read_text(encoding='utf-8'))
order=[20,21,22,23,24,30,31,32,33,10,11,12,13,14]
first=data['samples'][0]['at']
last=None
groups={}
transitions=[]
for r in data['samples']:
    c=r.get('control',{})
    key=(c.get('state'),c.get('activeSkill'))
    if key!=last:
        transitions.append({'seconds':round(r['at']-first,2),'state':key[0],'skill':key[1],'command':c.get('commandTwist')})
        last=key
    if c.get('state')!='policy':continue
    rows={v['id']:v for v in r['servos']}
    angles=[]
    for id in order:
        refs=cal['joints']['references']; directions=cal['joints'].get('directions',{})
        angles.append((rows[id]['position']-refs[str(id)])*360/4096*directions.get(str(id),-1))
    r['angles']=angles
    groups.setdefault(c.get('activeSkill'),[]).append(r)
print('TRANSITIONS',json.dumps(transitions,ensure_ascii=False))
result={'transitions':transitions,'groups':{}}
for skill,rows in groups.items():
    print('\nSKILL',skill,'samples',len(rows))
    for r in rows[::max(1,len(rows)//8)]:
        c=r['control']; targets=c.get('sentTargetDegrees',[])
        print(round(r['at']-first,2),'progress',c.get('skillProgressSeconds'),'command',c.get('commandTwist'),'actual', [round(v,1) for v in r['angles']], 'target',[round(v,1) for v in targets], 'holding',c.get('feedbackHolding'))
    summary={
        'samples':len(rows),'sha256':list({r['control'].get('policySha256') for r in rows}),
        'commandTwists':list({tuple(r['control'].get('commandTwist',[])) for r in rows}),
        'pausedSamples':sum(r['control'].get('feedbackHolding')==True for r in rows),
        'voltageRange':[min(s['voltage'] for r in rows for s in r['servos']),max(s['voltage'] for r in rows for s in r['servos'])],
        'maxLoad':max(abs(s['load']) for r in rows for s in r['servos']),
        'lastActualDeg':rows[-1]['angles'],'lastTargetDeg':rows[-1]['control'].get('sentTargetDegrees'),
        'actualRangeDeg':[[min(r['angles'][i] for r in rows),max(r['angles'][i] for r in rows)] for i in range(14)],
        'targetRangeDeg':[[min(r['control']['sentTargetDegrees'][i] for r in rows),max(r['control']['sentTargetDegrees'][i] for r in rows)] for i in range(14)],
    }
    result['groups'][skill]=summary
    print('SUMMARY',json.dumps(summary,ensure_ascii=False))
(directory/'trajectory-analysis.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
