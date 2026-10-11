import hashlib,json
from pathlib import Path
import numpy as np
import onnxruntime as ort
folder=Path(__file__).resolve().parent
trace=json.loads((folder/'policy-session-latest.json').read_text(encoding='utf-8'))
frames=[f for f in trace['frames'] if f.get('type')=='inference']
assert all(f['kind']=='xgoduck_getup' for f in frames)
obs=np.array([f['observation'] for f in frames]);times=np.array([f['mono'] for f in frames]);times-=times[0]
tilt=np.degrees(np.arccos(np.clip(-obs[:,5],-1,1)))
ids=trace['jointIds'];names=trace['jointOrder'];cal=trace['calibration']['joints']
goals=np.array([f['sentTargetDegrees'] for f in frames]);actual=[];load=[];voltage=[];faults=[]
for f in frames:
    positions=[];loads=[];volts=[]
    for id in ids:
        row=f['feedbackAfterWrite'][str(id)]
        sign=cal['directions'].get(str(id),-1)
        positions.append((row['position']-cal['references'][str(id)])*360/4096/sign)
        loads.append(row['load']);volts.append(row['voltage'])
        if row['fault']:faults.append([f['mono'],id,row['fault']])
    actual.append(positions);load.append(loads);voltage.append(volts)
actual=np.array(actual);load=np.array(load);voltage=np.array(voltage)
error=np.abs(goals-actual)
observer=folder.parents[5]
session=ort.InferenceSession(str(observer/'models/xgoduck_getup.onnx'),providers=['CPUExecutionProvider'])
raw=np.array([f['rawActionRadians'] for f in frames]);replayed=np.concatenate([session.run(None,{session.get_inputs()[0].name:x.astype(np.float32)[None]})[0] for x in obs])
np.testing.assert_allclose(raw,replayed,atol=1e-6,rtol=0)
failure=json.loads((folder/'policy-failure-latest.json').read_text(encoding='utf-8'))
rows=[{'id':id,'joint':name,'maxTrackingErrorDeg':float(error[:,i].max()),'p95TrackingErrorDeg':float(np.percentile(error[:,i],95)),'maxAbsLoadRaw':int(np.abs(load[:,i]).max()),'lastTargetDeg':float(goals[-1,i]),'lastActualDeg':float(actual[-1,i]),'saturatedFrames':sum(id in f['saturatedIds'] for f in frames)} for i,(id,name) in enumerate(zip(ids,names))]
result={'policySha256':hashlib.sha256((observer/'models/xgoduck_getup.onnx').read_bytes()).hexdigest(),'frames':len(frames),'recordedWallDurationSeconds':float(times[-1]),'effectiveSeconds':trace['result']['skillProgressSeconds'],'result':trace['result']['state'],'endMessage':trace['result']['message'],'tiltDegrees':{'first':float(tilt[0]),'min':float(tilt.min()),'last':float(tilt[-1]),'max':float(tilt.max())},'gyroMaxRadSec':np.abs(obs[:,:3]).max(axis=0).tolist(),'voltageRange':[float(voltage.min()),float(voltage.max())],'faults':faults,'missingFrames':sum(bool(f['readDiagnostics'].get('missingIds')) for f in frames),'transitionFrames':[f for f in trace['frames'] if f['type']!='inference'],'perJoint':rows,'onnxRawReplayMaxErrorRad':float(np.abs(raw-replayed).max()),'savedFault':{'error':failure['error'],'eventTimeMs':failure['eventTimeMs'],'stage':failure['stage'],'activeSkill':failure.get('stats',{}).get('activeSkill')},'sessionEventTimeMs':trace['eventTimeMs'],'hardwareMotionAdded':False,'sourceHashes':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in folder.iterdir() if p.is_file() and p.name!='analysis.json'}}
(folder/'analysis.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k not in ('sourceHashes','perJoint')},ensure_ascii=False,indent=2))
print(json.dumps(sorted(rows,key=lambda r:r['p95TrackingErrorDeg'],reverse=True)[:6],ensure_ascii=False,indent=2))
