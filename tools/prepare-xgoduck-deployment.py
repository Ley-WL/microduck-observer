"""Build donor parity fixture and reuse the checked read-only release installer."""
import json
from pathlib import Path
import numpy as np
import onnxruntime as ort

root=Path(__file__).resolve().parents[1]
fixture=json.loads((root/'backend-rust/tests/fixtures/policy-walk-v6.json').read_text())
fixture.update(kind='xgoduck',speed=0.)
fixture['observation'][48:51]=[0.,0.,0.]
model=root/'debug-server/models/xgoduck_walk.onnx'
session=ort.InferenceSession(str(model),providers=['CPUExecutionProvider'])
fixture['action']=session.run(None,{session.get_inputs()[0].name:np.asarray([fixture['observation']],dtype=np.float32)})[0][0].tolist()
(root/'backend-rust/tests/fixtures/policy-xgoduck.json').write_text(json.dumps(fixture,indent=2),encoding='utf-8')
source=(root/'deploy/install-model-walk.py').read_text(encoding='utf-8')
for old,new in [('20261004-model-walk-v1','20261007-xgoduck-hold-v1'),('hd1910-walk-v6-symmetry500','xgoduck_walk'),('policy-walk-v6.json','policy-xgoduck.json'),('observer-model-walk.new','observer-xgoduck.new'),('model-walk-frontend.tar.gz','xgoduck-frontend.tar.gz'),('frontend.pre-model-walk-v1','frontend.pre-xgoduck-hold-v1'),('current.model-walk-next','current.xgoduck-next'),('model-walk-deployment.json','xgoduck-deployment.json')]:
    source=source.replace(old,new)
(root/'deploy/install-xgoduck-hold.py').write_text(source,encoding='utf-8')
