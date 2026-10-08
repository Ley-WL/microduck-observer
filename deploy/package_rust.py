"""Package the native ARM64 service; no Python runtime, state or credentials."""
import argparse
import hashlib
import json
from pathlib import Path
import tarfile

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--frontend', type=Path, default=root / 'frontend/dist')
parser.add_argument('--output', type=Path, default=root / 'deploy/staging/observer-rust.tar.gz')
args = parser.parse_args()
header = args.binary.read_bytes()[:20]
if header[:4] != b'\x7fELF' or int.from_bytes(header[18:20], 'little') != 183:
    raise SystemExit('Expected a Linux ARM64 ELF executable')
if not (args.frontend / 'index.html').is_file():
    raise SystemExit('Provide the unchanged frontend dist via --frontend')
models=[]
for name in ('hd1910-head-v5','hd1910-walk-v6-symmetry500','xgoduck_walk',
             'xgoduck_getup','xgoduck_pick','xgoduck_roulade','alpha_sitstand'):
    model=root / 'debug-server/models' / (name+'.onnx')
    metadata=model.with_suffix('.metadata.json')
    if hashlib.sha256(model.read_bytes()).hexdigest() != json.loads(metadata.read_text())['policySha256']:
        raise SystemExit('Model SHA256 mismatch: '+name)
    models.append((model,metadata))
args.output.parent.mkdir(parents=True, exist_ok=True)
with tarfile.open(args.output, 'w:gz') as archive:
    info = archive.gettarinfo(str(args.binary), arcname='bin/microduck-observer')
    info.mode = 0o755
    with args.binary.open('rb') as stream:
        archive.addfile(info, stream)
    archive.add(args.frontend, arcname='frontend/dist')
    for model,metadata in models:
        archive.add(model, arcname='models/' + model.name)
        archive.add(metadata, arcname='models/' + metadata.name)
    archive.add(root / 'deploy/microduck-observer-rust.service', arcname='deploy/microduck-observer-rust.service')
print(args.output)
