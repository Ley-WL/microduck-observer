"""Deploy model HOME control and frontend; preserve model, calibration and servo state."""
from pathlib import Path
import subprocess
import tarfile

root=Path(__file__).resolve().parents[1]
archive=root/'frontend/model-walk-frontend.tar.gz'
with tarfile.open(archive,'w:gz') as tar:
    tar.add(root/'frontend/dist',arcname='dist')
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
for source,target in [(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer','observer-model-walk.new'),
                      (root/'deploy/install-model-walk.py','install-model-walk.py'), (archive,'model-walk-frontend.tar.gz'),
                      (root/'debug-server/models/hd1910-walk-v6-symmetry500.onnx','hd1910-walk-v6-symmetry500.onnx'),
                      (root/'debug-server/models/hd1910-walk-v6-symmetry500.metadata.json','hd1910-walk-v6-symmetry500.metadata.json'),
                      (root/'backend-rust/tests/fixtures/policy-walk-v6.json','policy-walk-v6.json')]:
    subprocess.run(['scp',*opts,str(source),f'{host}:/home/radxa/{target}'],check=True)
password=next(line.removeprefix('Password: ') for line in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/install-model-walk.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
directory=root/'docs/实测记录/附件/HAT/2026-10-04/model-walk';directory.mkdir(parents=True,exist_ok=True)
subprocess.run(['scp',*opts,f'{host}:/home/radxa/model-walk-deployment.json',str(directory/'deployment.json')],check=True)
result.check_returncode()
archive.unlink(missing_ok=True)
