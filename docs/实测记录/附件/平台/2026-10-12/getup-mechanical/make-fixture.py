import json
from pathlib import Path
import onnxruntime as ort,numpy as np
p=Path('/tmp/getup-mechanical-fixture.json');f=json.loads(p.read_text())
s=ort.InferenceSession('/home/leiwu30/microduck-training/getup-mechanical-train-20261012/evidence/mechanical-training/train.onnx',providers=['CPUExecutionProvider'])
f['action']=s.run(None,{s.get_inputs()[0].name:np.array(f['observation'],dtype=np.float32)[None]})[0].reshape(-1).tolist()
p.write_text(json.dumps(f,indent=2))
