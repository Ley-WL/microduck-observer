import json, math, hashlib, collections, sys
from pathlib import Path
import numpy as np
root=Path(__file__).resolve().parent
name=sys.argv[1] if len(sys.argv)>1 else 'session.json'
source=root/name
d=json.loads(source.read_text(encoding='utf-8'))
groups=collections.defaultdict(list)
for f in d['frames']:
    if f.get('type')=='inference':groups[f['kind']].append(f)
out={'sessionSha256':hashlib.sha256(source.read_bytes()).hexdigest(),
     'sessionResult':d['result'],'motionIssued':False,'groups':{}}
failure_source=root/('failure.json' if name=='session.json' else 'getup-failure.json')
failure=json.loads(failure_source.read_text(encoding='utf-8'))
out['downloadedFailure']={'sha256':hashlib.sha256(failure_source.read_bytes()).hexdigest(),
                         'eventTimeMs':failure.get('eventTimeMs'),
                         'sessionEventTimeMs':d.get('eventTimeMs'),
                         'appliesToThisSession':failure.get('eventTimeMs')==d.get('eventTimeMs')}
def tilt(f):
    gravity=np.array(f['observation'][3:6],dtype=float)
    return math.degrees(math.acos(np.clip(-gravity[2]/np.linalg.norm(gravity),-1,1)))
cal=d['calibration']['joints']
for kind,frames in groups.items():
    entry={'frames':len(frames),'head':{},'firstMono':frames[0]['mono'],'lastMono':frames[-1]['mono']}
    entry['tiltDeg']={'first':tilt(frames[0]),'last':tilt(frames[-1]),
                      'min':min(map(tilt,frames)), 'max':max(map(tilt,frames))}
    for name in ['neck_pitch','head_pitch','head_yaw','head_roll']:
        n=d['jointOrder'].index(name);sid=str(d['jointIds'][n])
        refs=cal['references'][sid];direction=cal['directions'].get(sid,-1)
        rows=[f['feedbackBeforeWrite'][sid] for f in frames]
        measured=[(r['position']-refs)*360/4096*direction for r in rows]
        targets=[f['sentTargetDegrees'][n] for f in frames]
        assert all(abs((f['goals'][sid]-refs)*360/4096*direction-f['sentTargetDegrees'][n])<1e-6 for f in frames)
        errors=[abs(a-b) for a,b in zip(targets,measured)]
        hardware_goals=[(r['goalPositionRaw']-refs)*360/4096*direction for r in rows]
        entry['head'][name]={'id':int(sid),'gains':sorted({(r['kpRaw'],r['kdRaw']) for r in rows}),
            'targetRangeDeg':[min(targets),max(targets)],'measuredRangeDeg':[min(measured),max(measured)],
            'lastTargetDeg':targets[-1],'lastMeasuredDeg':measured[-1],
            'lastHardwareGoalDeg':(rows[-1]['goalPositionRaw']-refs)*360/4096*direction,
            'instantErrorP95Deg':float(np.percentile(errors,95)),
            'last10ErrorMedianDeg':float(np.median(errors[-10:])),
            'homeDeg':frames[-1]['homeRadians'][n]*180/math.pi,
            'clippedFrames':sum(abs(f['preClipTargetDegrees'][n]-f['sentTargetDegrees'][n])>.05 for f in frames),
            'voltageRange':[min(r['voltage'] for r in rows),max(r['voltage'] for r in rows)],
            'maxAbsLoad':max(abs(r['load']) for r in rows),'maxAbsCurrentRaw':max(abs(r['currentRaw']) for r in rows),
            'faults':sorted({r['fault'] for r in rows}),'torqueStates':sorted({r['torque'] for r in rows})}
        tail_count=min(50,len(frames)-1)
        entry['head'][name]['lastSecond']={
            'targetRangeDeg':[min(targets[-50:]),max(targets[-50:])],
            'measuredRangeDeg':[min(measured[-50:]),max(measured[-50:])],
            'targetStepP95Deg':float(np.percentile(np.abs(np.diff(targets[-50:])),95)),
            'previousSentTargetErrorMedianDeg':float(np.median(np.abs(np.array(targets[-tail_count-1:-1])-measured[-tail_count:]))),
            'hardwareGoalNote':'Cached profile register 42; not a per-frame target readback and not suitable for tracking-error measurement'}
        entry['head'][name]['samples']=[{'frame':i,'elapsedSec':frames[i]['mono']-frames[0]['mono'],
            'tiltDeg':tilt(frames[i]),'targetDeg':targets[i], 'hardwareGoalDeg':hardware_goals[i],
            'measuredDeg':measured[i], 'kp':rows[i]['kpRaw'],'kd':rows[i]['kdRaw']}
            for i in sorted(set(list(range(0,len(frames),25))+list(range(max(0,len(frames)-10),len(frames)))))]
    out['groups'][kind]=entry
if source.name=='getup-session.json':
    donor=json.loads((root.parent/'getup-compare'/'donor-stand-model-info.json').read_text(encoding='utf-8'))
    out['homeComparison']={}
    for joint in ['neck_pitch','head_pitch']:
        n=d['jointOrder'].index(joint);sid=str(d['jointIds'][n]);direction=cal['directions'].get(sid,-1)
        home=frames[-1]['homeRadians'][n]*180/math.pi
        donor_deg=donor['home'][joint]*180/math.pi
        out['homeComparison'][joint]={'ourHomeDeg':home,'donorHomeDeg':donor_deg,
            'ourHomeMatchesDonor':abs(home-donor_deg)<1e-6,
            'actualAtHomeCompletionApproxDeg':home+direction*d['result']['positionErrorsDeg'][sid],
            'errorConvention':'result error is raw position minus raw target; multiply calibrated direction'}
destination='analysis.json' if source.name=='session.json' else source.stem+'-analysis.json'
(root/destination).write_text(json.dumps(out,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(out,ensure_ascii=False))
