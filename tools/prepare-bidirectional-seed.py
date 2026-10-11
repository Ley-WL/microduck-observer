"""Import the user-selected supervised actor, preserving honest validation scope."""
import hashlib
import json
import shutil
from pathlib import Path
import numpy as np
import onnxruntime as ort

root = Path(__file__).resolve().parents[1]
source = root.parent / 'microduck-rl/artifacts/sit-contact-bidirectional-20261010'
expected = '2a3edcb880e88435a7ac248568859bb4f1eb487b9204db06a4aa3fb59c5ebe29'
model = source / 'single-actor-seed.onnx'
meta = json.loads((source / 'single-actor-seed.metadata.json').read_text())
assert hashlib.sha256(model.read_bytes()).hexdigest() == expected == meta['policySha256']
backup = root / 'deploy/staging/bidirectional-seed-20261010/previous-model'
backup.mkdir(parents=True, exist_ok=True)
for suffix in ('onnx', 'metadata.json'):
    target = root / 'models' / ('alpha_sitstand.' + suffix)
    saved = backup / target.name
    if not saved.exists():
        shutil.copy2(target, saved)
shutil.copy2(model, root / 'models/alpha_sitstand.onnx')
meta['deploymentProvenance'] = {
    'candidate': 'single-actor-supervised-seed-before-PPO',
    'userSelected': True, 'hardwareValidated': False,
    'cpuScreening': {'cycles': [31, 32], 'standHold': [14, 16], 'sitHold': [16, 16]},
    'formalAcceptancePassed': False,
    'note': 'Small-sample screening; not final PPO weights or dual-expert graph.'
}
(root / 'models/alpha_sitstand.metadata.json').write_text(json.dumps(meta, indent=2) + '\n')
session = ort.InferenceSession(str(model), providers=['CPUExecutionProvider'])
for name in ('sit', 'stand'):
    path = root / f'backend-rust/tests/fixtures/policy-sitstand-{name}.json'
    fixture = json.loads(path.read_text())
    observation = np.asarray([fixture['observation']], dtype=np.float32)
    fixture['action'] = session.run(None, {session.get_inputs()[0].name: observation})[0].reshape(-1).tolist()
    path.write_text(json.dumps(fixture, indent=2) + '\n')
print(json.dumps({'policySha256': expected, 'fixtures': 2, 'hardwareAccess': False}))
