"""Deploy only policy fault confirmation backend; preserve frontend, model and servo state."""
from pathlib import Path
import subprocess

root=Path(__file__).resolve().parents[1]
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
import tarfile
archive=root/'deploy/sitstand-frontend.tar.gz'
with tarfile.open(archive,'w:gz') as out:out.add(root/'frontend/dist',arcname='dist')
for source,target in [(archive,'sitstand-frontend.tar.gz'),(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer','observer-sitstand.new'),
 (root/'debug-server/models/alpha_sitstand.onnx','alpha_sitstand.onnx'),(root/'debug-server/models/alpha_sitstand.metadata.json','alpha_sitstand.metadata.json'),
 (root/'backend-rust/tests/fixtures/policy-sitstand-stand.json','policy-sitstand-stand.json'),(root/'backend-rust/tests/fixtures/policy-sitstand-sit.json','policy-sitstand-sit.json'),
 (root/'deploy/install-official-sitstand.py','install-official-sitstand.py')]:
    subprocess.run(['scp',*opts,str(source),f'{host}:/home/radxa/{target}'],check=True)
password=next(line.removeprefix('Password: ') for line in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/install-official-sitstand.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
directory=root/'docs/实测记录/附件/平台/2026-10-08/official-sitstand';directory.mkdir(parents=True,exist_ok=True)
subprocess.run(['scp',*opts,f'{host}:/home/radxa/sitstand-deployment.json',str(directory/'deployment.json')],check=True)
result.check_returncode()
