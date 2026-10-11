import hashlib,json
from pathlib import Path
import numpy as np
import onnxruntime as ort

folder=Path(__file__).resolve().parent;root=folder.parents[5]
path=folder/'getup-1791685669674.json'
trace=json.loads(path.read_text(encoding='utf-8'))
frames=[f for f in trace['frames'] if f['type']=='inference']
assert len(frames)==378 and all(f['kind']=='xgoduck_getup' for f in frames)
ids=trace['jointIds'];cal=trace['calibration']['joints'];obs=np.array([f['observation'] for f in frames]);t=np.array([f['mono'] for f in frames]);t-=t[0]
tilt=np.degrees(np.arccos(np.clip(-obs[:,5]/np.linalg.norm(obs[:,3:6],axis=1),-1,1)))
targets=np.array([f['sentTargetDegrees'] for f in frames]);raw_targets=np.array([f['preClipTargetDegrees'] for f in frames])
actual=[];loads=[];volts=[];faults=[]
for f in frames:
    rows=[f['feedbackAfterWrite'][str(id)] for id in ids]
    actual.append([(r['position']-cal['references'][str(id)])*360/4096*cal['directions'].get(str(id),-1) for id,r in zip(ids,rows)])
    loads.append([r['load'] for r in rows]);volts.append([r['voltage'] for r in rows]);faults.append([r['fault'] for r in rows])
actual=np.array(actual);loads=np.array(loads);error=np.abs(targets-actual)
mechanical=json.loads((root/'backend-rust/tests/fixtures/kissqy-mechanical-limits.json').read_text())
bounds={j['id']:np.degrees([j['min_rad'],j['max_rad']]) for j in mechanical['joints']}
session=ort.InferenceSession(str(root/'models/xgoduck_getup.onnx'),providers=['CPUExecutionProvider'])
replayed=np.concatenate([session.run(None,{session.get_inputs()[0].name:x.astype(np.float32)[None]})[0] for x in obs])
raw=np.array([f['rawActionRadians'] for f in frames]);max_error=float(np.abs(raw-replayed).max());assert max_error<1e-6
rows=[]
for n,(id,name) in enumerate(zip(ids,trace['jointOrder'])):
    lo,hi=bounds[id]
    rows.append({'id':id,'name':name,'mechanicalDegrees':[lo,hi],'sentRangeDegrees':[float(targets[:,n].min()),float(targets[:,n].max())],
                 'actualRangeDegrees':[float(actual[:,n].min()),float(actual[:,n].max())],
                 'targetOutsideMechanicalFrames':int(np.sum((targets[:,n]<lo-.05)|(targets[:,n]>hi+.05))),
                 'saturatedFrames':sum(id in f['saturatedIds'] for f in frames),
                 'trackingErrorP95Degrees':float(np.percentile(error[:,n],95)),
                 'trackingErrorMaxDegrees':float(error[:,n].max()),
                 'loadAbsP95Raw':float(np.percentile(np.abs(loads[:,n]),95)),
                 'loadAbsMaxRaw':int(np.abs(loads[:,n]).max()),
                 'loadAbsAbove950Frames':int(np.sum(np.abs(loads[:,n])>=950)),
                 'targetTravelDegrees':float(np.abs(np.diff(targets[:,n])).sum()),
                 'actualTravelDegrees':float(np.abs(np.diff(actual[:,n])).sum()),
                 'lastTargetDegrees':float(targets[-1,n]),'lastActualDegrees':float(actual[-1,n]),
                 'firstTargetDegrees':float(targets[0,n]),'firstActualDegrees':float(actual[0,n])})
assert not any(r['targetOutsideMechanicalFrames'] for r in rows)
best=int(np.argmin(tilt));i6=int(np.searchsorted(t,6))
result={'eventTimeMs':trace['eventTimeMs'],'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'modelSha256':frames[0]['policySha256'],'frames':len(frames),
        'durationSeconds':float(t[-1]),'effectiveSeconds':trace['result']['skillProgressSeconds'],'resultState':trace['result']['state'],
        'tiltDegrees':{'first':float(tilt[0]),'best':float(tilt[best]),'bestAtSeconds':float(t[best]),'last':float(tilt[-1]),'at6Seconds':float(tilt[i6])},
        'inferenceIntervalsMs':{'median':float(np.median(np.diff(t))*1000),'p95':float(np.percentile(np.diff(t),95)*1000),'max':float(np.diff(t).max()*1000)},
        'resultAgeMs':{'median':float(np.median([f['resultAgeMs'] for f in frames])),'p95':float(np.percentile([f['resultAgeMs'] for f in frames],95))},
        'rates':trace['result'].get('cycleCosts'),'voltageRange':[float(np.min(volts)),float(np.max(volts))],'faultFrames':int(np.sum(np.any(np.array(faults)!=0,axis=1))),
        'missingFrames':sum(bool(f['readDiagnostics']['missingIds']) for f in frames),'onnxReplayMaxErrorRadians':max_error,
        'after6EffectiveSecondsFrames':len(frames)-300,'transitions':[f for f in trace['frames'] if f['type']!='inference'],
        'hardwareMotionAdded':False,'perJoint':rows}
(folder/'new-getup-analysis.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
np.savez_compressed(folder/'new-getup-aligned.npz',time=t,tilt=tilt,targets=targets,actual=actual,loads=loads,rawTargets=raw_targets,observations=obs)
print(json.dumps({k:v for k,v in result.items() if k not in ['rates','perJoint','transitions']},ensure_ascii=False,indent=2))
print(json.dumps(sorted(rows,key=lambda r:r['trackingErrorP95Degrees'],reverse=True)[:7],ensure_ascii=False,indent=2))
