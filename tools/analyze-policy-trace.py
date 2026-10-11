"""Verify exact captured ONNX inputs/output and quantify transition oscillation."""
import argparse,json,hashlib
from pathlib import Path
import numpy as np
import onnxruntime as ort
root=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser()
p.add_argument('--input',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
a=p.parse_args()
trace=json.loads(a.input.read_text(encoding='utf-8'))
sessions={}
report={'hardwareMotionAdded':False,'frames':len(trace['frames']),'transitions':[], 'groups':[]}
current=None
for frame in trace['frames']:
    if frame['type']!='inference':
        report['transitions'].append(frame)
        current=None
        continue
    kind=frame['kind']
    if kind not in ('sitstand_sit','sitstand_stand'):continue
    if current is None:
        current={'kind':kind,'startMono':frame['mono'],'frames':0,'onnxMaxError':0.,'emaMaxError':0.,
                 'rawTargets':[],'filteredTargets':[],'times':[],'resultAgesMs':[]}
        report['groups'].append(current)
    model=root/'models/alpha_sitstand.onnx'
    assert hashlib.sha256(model.read_bytes()).hexdigest()==frame['policySha256']
    if str(model) not in sessions:
        options=ort.SessionOptions();options.intra_op_num_threads=1
        sessions[str(model)]=ort.InferenceSession(str(model),sess_options=options,providers=['CPUExecutionProvider'])
    session=sessions[str(model)]
    obs=np.asarray(frame['observation'],dtype=np.float32)
    raw=np.asarray(frame['rawActionRadians'],dtype=np.float32)
    replay=session.run(None,{session.get_inputs()[0].name:obs[None]})[0].reshape(-1)
    previous=np.asarray(frame['emaPreviousRadians'],dtype=np.float32)
    filtered=np.asarray(frame['filteredActionRadians'],dtype=np.float32)
    home=np.asarray(frame['homeRadians'],dtype=np.float32)
    current['onnxMaxError']=max(current['onnxMaxError'],float(np.max(np.abs(raw-replay))))
    current['emaMaxError']=max(current['emaMaxError'],float(np.max(np.abs(filtered-(np.float32(.45)*previous+np.float32(.55)*raw)))))
    current['frames']+=1
    current['times'].append(frame['mono']-current['startMono'])
    current['rawTargets'].append(np.degrees(home+raw).tolist())
    current['filteredTargets'].append(frame['sentTargetDegrees'])
    current['resultAgesMs'].append(frame['resultAgeMs'])
for group in report['groups']:
    indices=[i for i,t in enumerate(group['times']) if t<2.]
    raw=np.array(group.pop('rawTargets'))[indices]
    target=np.array(group.pop('filteredTargets'))[indices]
    group.pop('times')
    ages=group.pop('resultAgesMs')
    group['meanResultAgeMs']=float(np.mean(ages));group['maxResultAgeMs']=max(ages)
    group['firstTwoSecondsFrames']=len(indices)
    group['rawKneeRangeDeg']=np.stack([raw[:,[3,12]].min(axis=0),raw[:,[3,12]].max(axis=0)],axis=1).tolist()
    group['sentKneeRangeDeg']=np.stack([target[:,[3,12]].min(axis=0),target[:,[3,12]].max(axis=0)],axis=1).tolist()
    group['rawKneeTotalVariationDeg']=np.abs(np.diff(raw[:,[3,12]],axis=0)).sum(axis=0).tolist()
    group['sentKneeTotalVariationDeg']=np.abs(np.diff(target[:,[3,12]],axis=0)).sum(axis=0).tolist()
    group['exactReplayPassed']=group['onnxMaxError']<1e-5 and group['emaMaxError']<1e-6
report['limitations']='Exact-input replay verifies runtime output; it does not prove changing history/delay improves a closed-loop physical trajectory.'
a.output.write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps(report,ensure_ascii=False))
