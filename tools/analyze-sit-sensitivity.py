"""Fixed-feedback sensitivity only; cannot establish a closed-loop fix."""
import json
from pathlib import Path
import numpy as np
import onnxruntime as ort
root=Path(__file__).resolve().parents[1]
p=root/'docs/实测记录/附件/平台/2026-10-10/sit-trace'
trace=json.loads((p/'session.json').read_text(encoding='utf-8'))
frames=[r for r in trace['frames'] if r['type']=='inference' and r['kind']=='sitstand_sit']
frames=[r for r in frames if r['mono']<frames[0]['mono']+2]
options=ort.SessionOptions();options.intra_op_num_threads=1
session=ort.InferenceSession(str(root/'models/alpha_sitstand.onnx'),sess_options=options,providers=['CPUExecutionProvider'])
results={}
for name in ('baseline','gyro-zero','velocity-zero','raw-history-zero','gravity-upright'):
    actions=[]
    for row in frames:
        obs=np.array(row['observation'],dtype=np.float32)
        if name=='gyro-zero':obs[:3]=0
        if name=='velocity-zero':obs[20:34]=0
        if name=='raw-history-zero':obs[34:48]=0
        if name=='gravity-upright':obs[3:6]=[0,0,-1]
        actions.append(session.run(None,{session.get_inputs()[0].name:obs[None]})[0].reshape(-1))
    target=np.degrees(np.array(actions)+np.array(frames[0]['homeRadians']))
    results[name]={'kneeTotalVariationDeg':np.abs(np.diff(target[:,[3,12]],axis=0)).sum(axis=0).tolist(),
                   'kneeRangeDeg':np.stack([target[:,[3,12]].min(axis=0),target[:,[3,12]].max(axis=0)],axis=1).tolist()}
report={'frames':len(frames),'hardwareMotionAdded':False,
    'limitations':'Fixed real position/other observation sequence; replacing one block tests sensitivity, not whether that block is incorrect or whether intervention fixes physical closed loop.',
    'results':results}
(p/'sensitivity.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
