import json, math, hashlib, collections
from pathlib import Path
import numpy as np
root=Path(__file__).resolve().parent
d=json.loads((root/'session.json').read_text(encoding='utf-8'))
groups=collections.defaultdict(list)
for f in d['frames']:
    if f.get('type')=='inference':groups[f['kind']].append(f)
out={'sessionSha256':hashlib.sha256((root/'session.json').read_bytes()).hexdigest(),
     'failureSha256':hashlib.sha256((root/'failure.json').read_bytes()).hexdigest(),
     'sessionResult':d['result'],'motionIssued':False,'groups':{}}
cal=d['calibration']['joints']
for kind,frames in groups.items():
    entry={'frames':len(frames),'head':{}}
    entry['tiltDeg']={'first':math.degrees(math.acos(np.clip(-frames[0]['observation'][2],-1,1))),
                      'last':math.degrees(math.acos(np.clip(-frames[-1]['observation'][2],-1,1)))}
    for name in ['neck_pitch','head_pitch','head_yaw','head_roll']:
        n=d['jointOrder'].index(name);sid=str(d['jointIds'][n])
        refs=cal['references'][sid];direction=cal['directions'].get(sid,-1)
        rows=[f['feedbackBeforeWrite'][sid] for f in frames]
        measured=[(r['position']-refs)*360/4096*direction for r in rows]
        targets=[f['sentTargetDegrees'][n] for f in frames]
        assert all(abs((f['goals'][sid]-refs)*360/4096*direction-f['sentTargetDegrees'][n])<1e-6 for f in frames)
        errors=[abs(a-b) for a,b in zip(targets,measured)]
        entry['head'][name]={'id':int(sid),'gains':sorted({(r['kpRaw'],r['kdRaw']) for r in rows}),
            'targetRangeDeg':[min(targets),max(targets)],'measuredRangeDeg':[min(measured),max(measured)],
            'lastTargetDeg':targets[-1],'lastMeasuredDeg':measured[-1],
            'lastHardwareGoalDeg':(rows[-1]['goalPositionRaw']-refs)*360/4096*direction,
            'instantErrorP95Deg':float(np.percentile(errors,95)),
            'last10ErrorMedianDeg':float(np.median(errors[-10:])),
            'clippedFrames':sum(abs(f['preClipTargetDegrees'][n]-f['sentTargetDegrees'][n])>.05 for f in frames),
            'voltageRange':[min(r['voltage'] for r in rows),max(r['voltage'] for r in rows)],
            'maxAbsLoad':max(abs(r['load']) for r in rows),'maxAbsCurrentRaw':max(abs(r['currentRaw']) for r in rows),
            'faults':sorted({r['fault'] for r in rows}),'torqueStates':sorted({r['torque'] for r in rows})}
    out['groups'][kind]=entry
(root/'analysis.json').write_text(json.dumps(out,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(out,ensure_ascii=False))
