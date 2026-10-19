import json,collections
from pathlib import Path
import numpy as np
import onnxruntime as ort
p=Path('.local/flamingo-jitter'); d=json.loads((p/'session.json').read_text(encoding='utf-8')); ids=d['jointIds']; names=d['jointOrder']; out={}
sess=ort.InferenceSession('models/flamingo_official.onnx',providers=['CPUExecutionProvider']); maxerr=0
for kind in ['xgoduck','flamingo_hold','flamingo_lower']:
 fs=[f for f in d['frames'] if f.get('kind')==kind and f.get('type')=='inference']; t=np.array([f['mono'] for f in fs]); dt=np.diff(t); ok=dt<.1
 obs=np.array([f['observation'] for f in fs]); target=np.array([f['sentTargetDegrees'] for f in fs]); pre=np.array([f['preClipTargetDegrees'] for f in fs]); actual=(obs[:,6:20]+np.array(fs[0]['homeRadians']))*180/np.pi
 steps=np.abs(np.diff(target,axis=0))[ok]; err=np.abs(target-actual); clip=np.abs(pre-target)>.1
 tilt=np.degrees(np.arccos(np.clip(-obs[:,5]/np.linalg.norm(obs[:,3:6],axis=1),-1,1)))
 top=np.argsort(np.percentile(steps,95,axis=0))[-5:][::-1]
 volt=[r['voltage'] for f in fs for r in f['feedbackBeforeWrite'].values()]
 r={'frames':len(fs),'hzContiguous':1/np.mean(dt[ok]),'maxIntervalMsContiguous':float(dt[ok].max()*1000),'captureGaps':dt[~ok].tolist(),'tiltMedianMax':np.percentile(tilt,[50,100]).tolist(),'minVoltage':min(volt),'missingFrames':sum(bool(f['readDiagnostics']['missingIds']) for f in fs),'checksumErrors':sum(f['readDiagnostics']['checksumErrors'] for f in fs),'inferenceP95Ms':float(np.percentile([f['inferenceMs'] for f in fs],95)),'topJoints':[{'id':ids[n],'name':names[n],'targetStepP95Deg':float(np.percentile(steps[:,n],95)),'trackingErrorP95Deg':float(np.percentile(err[:,n],95)),'clippedPercent':float(clip[:,n].mean()*100)} for n in top],'clipping':[{'id':ids[n],'percent':float(clip[:,n].mean()*100),'maxExcessDeg':float(abs(pre-target)[:,n].max())} for n in range(14) if clip[:,n].any()]}
 if kind.startswith('flamingo'):
  for f in fs:
   a=sess.run(None,{'obs':np.asarray([f['observation']],dtype=np.float32)})[0][0];maxerr=max(maxerr,float(np.max(np.abs(a-f['rawActionRadians']))))
  r['rawVsFilteredMax']=float(np.max(np.abs(np.array([f['rawActionRadians'] for f in fs])-np.array([f['filteredActionRadians'] for f in fs]))))
 out[kind]=r
out['onnxMaxError']=maxerr;out['eventTimeMs']=d['eventTimeMs'];out['failureStats']=json.loads((p/'failure.json').read_text(encoding='utf-8'))['stats'];(p/'analysis.json').write_text(json.dumps(out,indent=2),encoding='utf-8');print(json.dumps(out,indent=2))
