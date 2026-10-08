"""Deploy only policy fault confirmation backend; preserve frontend, model and servo state."""
from pathlib import Path
import subprocess

root=Path(__file__).resolve().parents[1]
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
for source,target in [(root/'frontend/public/model/model.json','xgoduck-model-ranges.json'),(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer','observer-xgoduck-ranges.new'),
                      (root/'deploy/install-xgoduck-ranges.py','install-xgoduck-ranges.py')]:
    subprocess.run(['scp',*opts,str(source),f'{host}:/home/radxa/{target}'],check=True)
password=next(line.removeprefix('Password: ') for line in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/install-xgoduck-ranges.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
directory=root/'docs/实测记录/附件/平台/2026-10-08/xgoduck-ranges';directory.mkdir(parents=True,exist_ok=True)
subprocess.run(['scp',*opts,f'{host}:/home/radxa/xgoduck-ranges-deployment.json',str(directory/'deployment.json')],check=True)
result.check_returncode()
