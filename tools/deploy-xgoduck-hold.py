"""Deploy requested donor model and hold-to-walk UI without starting robot motion."""
from pathlib import Path
import subprocess
import tarfile

root=Path(__file__).resolve().parents[1]
archive=root/'deploy/staging/xgoduck-frontend.tar.gz'
with tarfile.open(archive,'w:gz') as tar:
    tar.add(root/'frontend/dist',arcname='dist')
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
for source,target in [(root/'deploy/staging/observer-xgoduck.new','observer-xgoduck.new'),
                      (root/'deploy/install-xgoduck-hold.py','install-xgoduck-hold.py'),(archive,'xgoduck-frontend.tar.gz'),
                      (root/'models/xgoduck_walk.onnx','xgoduck_walk.onnx'),
                      (root/'models/xgoduck_walk.metadata.json','xgoduck_walk.metadata.json'),
                      (root/'backend-rust/tests/fixtures/policy-xgoduck.json','policy-xgoduck.json')]:
    subprocess.run(['scp',*opts,str(source),f'{host}:/home/radxa/{target}'],check=True)
password=next(line.removeprefix('Password: ') for line in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/install-xgoduck-hold.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
directory=root/'docs/实测记录/附件/平台/2026-10-07/xgoduck-hold';directory.mkdir(parents=True,exist_ok=True)
subprocess.run(['scp',*opts,f'{host}:/home/radxa/xgoduck-deployment.json',str(directory/'deployment.json')],check=True)
result.check_returncode()
