import json,hashlib
from pathlib import Path
import numpy as np
import onnxruntime as ort
p=Path('/home/radxa/microduck-observer/current/models/hd1910-head-v5.onnx')
m=json.loads(p.with_suffix('.metadata.json').read_text())
home=np.array(m['homeRadians'],dtype=np.float32)
ids=[20,21,22,23,24,30,31,32,33,10,11,12,13,14]
refs=[2072,2078,2024,2095,1967.2996566693441,2085,2022,1996,2069,2033,2070,2085,2018,2100.700343330656]
pos=[2073,2132,2322,2095,1670,2087,2020,1997,2070,2032,2015,1783,2022,2394]
q=(np.array(refs)-np.array(pos))*2*np.pi/4096
s=ort.InferenceSession(str(p),providers=['CPUExecutionProvider'])
results=[]
for name,angles in [('ideal_HOME',home),('recorded_static',q),('HOME_legs_static_head',np.r_[home[:5],q[5:9],home[9:]]),('static_legs_HOME_head',np.r_[q[:5],home[5:9],q[9:]])]:
 obs=np.zeros(61,dtype=np.float32);obs[5]=-1;obs[6:20]=angles-home
 action=s.run(None,{s.get_inputs()[0].name:obs[None]})[0].reshape(-1)
 results.append(dict(name=name,observation=obs.tolist(),actionRadians=action.tolist(),targetDegrees=((home+action)*180/np.pi).tolist(),changeDegrees=((home+action-angles)*180/np.pi).tolist()))
print(json.dumps(dict(hardwareAccess=False,synthetic=True,policySha256=hashlib.sha256(p.read_bytes()).hexdigest(),jointIds=ids,cases=results)))
