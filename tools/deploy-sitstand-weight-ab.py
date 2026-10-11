from pathlib import Path
import subprocess,shutil
root=Path(r'D:\Users\wl\Desktop\MicroDuck\microduck-observer')
stage=root/'deploy/staging/sitstand-weight-ab-20261010'
installer=stage/'install.py'
name='20261010-sitstand-weight-ab-v1'
remote='/home/radxa/sitstand-weight-ab-stage'
host='radxa@8.133.210.70';options=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=10','-o','ServerAliveCountMax=3','-o','HostKeyAlias=192.168.31.193']
ssh=['ssh',*options,'-p','5722',host];scp=['scp','-O',*options,'-P','5722']
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
files=[(installer,'install.py')]+[(stage/n,n) for n in ['alpha_sitstand.onnx','alpha_sitstand.metadata.json','policy-sitstand-sit.json','policy-sitstand-stand.json']]
files += [(root/'deploy/staging/kissqy-walk-20261010/policy-xgoduck.json','policy-xgoduck.json'),(root/'backend-rust/tests/fixtures/policy-xgoduck_getup.json','policy-xgoduck_getup.json')]
for source,target in files:subprocess.run([*scp,str(source),host+':'+remote+'/'+target],check=True)
credentials=Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt')
password=next(line.removeprefix('Password: ') for line in credentials.read_text().splitlines() if line.startswith('Password: '))
r=subprocess.run([*ssh,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python "+remote+'/install.py'],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(r.stdout,r.stderr,flush=True)
evidence=root/'docs/实测记录/附件/平台/2026-10-10/sitstand-weight-ab';evidence.mkdir(parents=True,exist_ok=True)
if r.returncode==0:
 subprocess.run([*scp,host+':/home/radxa/'+name+'-deployment.json',str(evidence/'deployment.json')],check=True)
 for direction in ('sit','stand'):shutil.copy2(stage/f'policy-sitstand-{direction}.json',root/f'backend-rust/tests/fixtures/policy-sitstand-{direction}.json')
r.check_returncode()
