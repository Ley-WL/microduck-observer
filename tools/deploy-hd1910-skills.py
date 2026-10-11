"""Upload reviewed model/service/frontend update and run read-only release installer."""
import argparse
import json
from pathlib import Path
import subprocess
import tarfile

root=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser()
p.add_argument('--host',default='8.133.210.70')
p.add_argument('--port',type=int,default=5722)
p.add_argument('--deployment-name',default='20261010-hd1910-skills-v1')
p.add_argument('--evidence-name',default='hd1910-skills')
a=p.parse_args()
assert all(c.isalnum() or c == '-' for c in a.deployment_name) and a.deployment_name
assert all(c.isalnum() or c == '-' for c in a.evidence_name) and a.evidence_name
host='radxa@'+a.host
ssh_opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','HostKeyAlias=192.168.31.193','-p',str(a.port)]
scp_opts=['-O','-o','BatchMode=yes','-o','ConnectTimeout=8','-o','HostKeyAlias=192.168.31.193','-P',str(a.port)]
stage=root/'deploy/staging/hd1910-skills-20261010'
stage.mkdir(parents=True,exist_ok=True)
frontend=stage/'frontend.tar.gz'
with tarfile.open(frontend,'w:gz') as archive:
    archive.add(root/'frontend/dist',arcname='dist')
files=[(frontend,'frontend.tar.gz'),
       (root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer','observer.new'),
       (root/'deploy/install-hd1910-skills.py','install.py')]
files += [(root/'models'/f'{name}.{suffix}',f'{name}.{suffix}')
          for name in ['xgoduck_getup','alpha_sitstand'] for suffix in ['onnx','metadata.json']]
files += [(root/'backend-rust/tests/fixtures'/name,name)
          for name in ['policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json']]
assert all(source.is_file() for source,_ in files)
subprocess.run(['ssh',*ssh_opts,host,'mkdir -p /home/radxa/hd1910-skills-stage'],check=True)
for source,target in files:
    subprocess.run(['scp',*scp_opts,str(source),host+':/home/radxa/hd1910-skills-stage/'+target],check=True)
password_file=Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt')
password=next(line.removeprefix('Password: ') for line in password_file.read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run(['ssh',*ssh_opts,host,"sudo -S -p '' env MICRODUCK_DEPLOYMENT_NAME="+a.deployment_name+" /home/radxa/microduck-observer/venv/bin/python /home/radxa/hd1910-skills-stage/install.py"],
                      input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
if result.returncode==0:
    evidence=root/'docs/实测记录/附件/平台/2026-10-10'/a.evidence_name
    evidence.mkdir(parents=True,exist_ok=True)
    subprocess.run(['scp',*scp_opts,host+':/home/radxa/'+a.deployment_name+'-deployment.json',str(evidence/'deployment.json')],check=True)
result.check_returncode()
