"""Deploy only policy fault confirmation backend; preserve frontend, model and servo state."""
from pathlib import Path
import subprocess

root=Path(__file__).resolve().parents[1]
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
for source,target in [(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer','observer-fault-confirm.new'),
                      (root/'deploy/install-policy-fault-confirm.py','install-policy-fault-confirm.py')]:
    subprocess.run(['scp',*opts,str(source),f'{host}:/home/radxa/{target}'],check=True)
password=next(line.removeprefix('Password: ') for line in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/install-policy-fault-confirm.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
directory=root/'docs/实测记录/附件/HAT/2026-10-07/policy-fault-confirm';directory.mkdir(parents=True,exist_ok=True)
subprocess.run(['scp',*opts,f'{host}:/home/radxa/fault-confirm-deployment.json',str(directory/'deployment.json')],check=True)
result.check_returncode()
