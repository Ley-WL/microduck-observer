"""Deploy XgoDuck skills, updating frontend and preserving existing models and servo state."""
from pathlib import Path
import subprocess
import tarfile

root=Path(__file__).resolve().parents[1]
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
subprocess.run(['ssh',*opts,host,'mkdir -p /home/radxa/ducklink-skills-stage'],check=True)
frontend_archive=root/'deploy/staging/xgoduck-skills-frontend.tar.gz'
with tarfile.open(frontend_archive,'w:gz') as archive:
    for entry in (root/'frontend/dist').iterdir():archive.add(entry,arcname=entry.name)
subprocess.run(['scp',*opts,str(frontend_archive),host+':/home/radxa/ducklink-skills-stage/frontend.tar.gz'],check=True)
for skill in ('getup','pick','roulade'):
    name='xgoduck_'+skill
    for suffix in ('onnx','metadata.json'):
        subprocess.run(['scp',*opts,str(root/'debug-server/models'/(name+'.'+suffix)),host+':/home/radxa/ducklink-skills-stage/'+name+'.'+suffix],check=True)
    fixture='policy-'+name+'.json'
    subprocess.run(['scp',*opts,str(root/'backend-rust/tests/fixtures'/fixture),host+':/home/radxa/ducklink-skills-stage/'+fixture],check=True)
for source,target in [(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer','observer-skills.new'),
                      (root/'deploy/install-xgoduck-skills.py','install-xgoduck-skills.py')]:
    subprocess.run(['scp',*opts,str(source),f'{host}:/home/radxa/{target}'],check=True)
password=next(line.removeprefix('Password: ') for line in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/install-xgoduck-skills.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
directory=root/'docs/实测记录/附件/平台/2026-10-08/xgoduck-skills';directory.mkdir(parents=True,exist_ok=True)
subprocess.run(['scp',*opts,f'{host}:/home/radxa/skills-deployment.json',str(directory/'deployment.json')],check=True)
result.check_returncode()
