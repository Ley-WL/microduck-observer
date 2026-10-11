from pathlib import Path
import hashlib,subprocess,shutil
root=Path(__file__).resolve().parents[1];stage=root/'deploy/staging/kissqy-pick-getup-20261010'
name='20261010-kissqy-pick-getup-v1';remote='/home/radxa/kissqy-pick-getup-stage'
binary=root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer'
t=(root/'deploy/staging/kissqy-sitstand-20261010/install.py').read_text(encoding='utf-8')
t=t.replace('20261010-kissqy-sitstand-v1',name).replace('/home/radxa/kissqy-sitstand-stage',remote)
t=t.replace("('alpha_sitstand',)","('xgoduck_pick','xgoduck_getup')")
t=t.replace("assert metadata['runtimeKind']=='kissqy_sitstand'","assert metadata['runtimeKind'] in ('kissqy_ground_pick','kissqy_stand')")
t=t.replace("assert metadata['policySha256']=='1c94934c6421ec72ed48079cea74966868a7a59f7d732e0207d0f7bdaeed3c50'","assert metadata['skillExecution']['gains']=={'p':5,'d':20}")
t='\n'.join(line for line in t.splitlines() if not line.startswith("assert manifest['binarySha256']=="))
t=t.replace("manifest['binarySha256'] = sha(release/'bin/microduck-observer')", "manifest['binarySha256'] = sha(release/'bin/microduck-observer')\nassert manifest['binarySha256']=="+repr(hashlib.sha256(binary.read_bytes()).hexdigest()))
t=t.replace("('policy-xgoduck.json','policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json')", "('policy-xgoduck.json','policy-xgoduck_pick.json','policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json')")
t=t.replace("[('xgoduck_getup','xgoduck_getup'),('sitstand_sit','alpha_sitstand'),('sitstand_stand','alpha_sitstand')]", "[('xgoduck_getup','xgoduck_getup'),('xgoduck_pick','xgoduck_pick')]")
installer=stage/'install.py';installer.write_text(t,encoding='utf-8');compile(t,str(installer),'exec')
host='radxa@192.168.31.186';options=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=10','-o','ServerAliveCountMax=3','-o','HostKeyAlias=192.168.31.193'];ssh=['ssh',*options,host];scp=['scp','-O',*options]
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
files=[(installer,'install.py'),(binary,'observer.new')]
for namefile in ['xgoduck_pick','xgoduck_getup']:
 files += [(stage/(namefile+'.'+ext),namefile+'.'+ext) for ext in ['onnx','metadata.json']]
 files += [(stage/('policy-'+namefile+'.json'),'policy-'+namefile+'.json')]
files += [(root/'deploy/staging/kissqy-walk-20261010/policy-xgoduck.json','policy-xgoduck.json')]
files += [(root/'deploy/staging/kissqy-sitstand-20261010'/('policy-sitstand-'+d+'.json'),'policy-sitstand-'+d+'.json') for d in ['sit','stand']]
for source,target in files:subprocess.run([*scp,str(source),host+':'+remote+'/'+target],check=True)
credentials=Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt')
password=next(line.removeprefix('Password: ') for line in credentials.read_text().splitlines() if line.startswith('Password: '))
r=subprocess.run([*ssh,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python "+remote+'/install.py'],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(r.stdout,r.stderr,flush=True)
evidence=root/'docs/实测记录/附件/平台/2026-10-10/kissqy-pick-getup';evidence.mkdir(parents=True,exist_ok=True)
if r.returncode==0:
 subprocess.run([*scp,host+':/home/radxa/'+name+'-deployment.json',str(evidence/'deployment.json')],check=True)
 for k in ['xgoduck_pick','xgoduck_getup']:shutil.copy2(stage/('policy-'+k+'.json'),root/'backend-rust/tests/fixtures'/('policy-'+k+'.json'))
r.check_returncode()
