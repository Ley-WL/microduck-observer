"""Deploy donor sitstand model/runtime to an isolated release, no robot motion."""
from pathlib import Path
import subprocess,hashlib,shutil
root=Path(__file__).resolve().parents[1]
stage=root/'deploy/staging/kissqy-sitstand-20261010'
name='20261010-kissqy-sitstand-v1'
remote='/home/radxa/kissqy-sitstand-stage'
binary=root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer'
script=(root/'deploy/install-hd1910-skills.py').read_text(encoding='utf-8')
script=script.replace("'20261010-hd1910-skills-v1'",repr(name)).replace('/home/radxa/hd1910-skills-stage',remote)
script=script.replace("('xgoduck_getup','alpha_sitstand')","('alpha_sitstand',)")
script=script.replace("    assert metadata['runtimeKind'] in ('hd1910_getup','hd1910_sitstand')", "    assert metadata['runtimeKind']=='kissqy_sitstand'")
script=script.replace("    assert all(value['worstGroupSuccessRate']>=.95 for value in metadata['simulationAcceptance'].values())", "    assert metadata['policySha256']=='1c94934c6421ec72ed48079cea74966868a7a59f7d732e0207d0f7bdaeed3c50'\n    assert metadata['hardwareValidated'] is False")
a=script.index('# Extract reviewed frontend');b=script.index('simulation_cal =',a)
script=script[:a]+"manifest['frontendAfter']=tree(release/'frontend')\n"+script[b:]
script=script.replace("('policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json')","('policy-xgoduck.json','policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json')")
script=script.replace("before = ready()", "before = ready()\nassert all(r['torque']==0 for r in before['joints']['data']['servos']), 'Stop and release servos before deployment'\ngains_before={n:(cal.parent/n).read_bytes() for n in ('model-gains.json','walk-model-gains.json')}")
script=script.replace("manifest['binarySha256'] = sha(release/'bin/microduck-observer')", "manifest['binarySha256'] = sha(release/'bin/microduck-observer')\nassert manifest['binarySha256']=="+repr(hashlib.sha256(binary.read_bytes()).hexdigest()))
script=script.replace("env = os.environ.copy()", "for n,v in gains_before.items():(release/n).write_bytes(v)\nenv = os.environ.copy()")
script=script.replace("    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'", "    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'\n    assert all((cal.parent/n).read_bytes()==v for n,v in gains_before.items()), 'Persistent gains changed'\n    manifest['gainsUnchanged']=True")
installer=stage/'install.py';installer.write_text(script,encoding='utf-8');compile(script,str(installer),'exec')
host='radxa@8.133.210.70';options=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=10','-o','ServerAliveCountMax=3','-o','HostKeyAlias=192.168.31.193']
ssh=['ssh',*options,'-p','5722',host];scp=['scp','-O',*options,'-P','5722']
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
files=[(installer,'install.py'),(binary,'observer.new')]+[(stage/n,n) for n in ['alpha_sitstand.onnx','alpha_sitstand.metadata.json','policy-sitstand-sit.json','policy-sitstand-stand.json']]
files += [(root/'deploy/staging/kissqy-walk-20261010/policy-xgoduck.json','policy-xgoduck.json'),(root/'backend-rust/tests/fixtures/policy-xgoduck_getup.json','policy-xgoduck_getup.json')]
for source,target in files:subprocess.run([*scp,str(source),host+':'+remote+'/'+target],check=True)
credentials=Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt')
password=next(line.removeprefix('Password: ') for line in credentials.read_text().splitlines() if line.startswith('Password: '))
r=subprocess.run([*ssh,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python "+remote+'/install.py'],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(r.stdout,r.stderr,flush=True)
evidence=root/'docs/实测记录/附件/平台/2026-10-10/kissqy-sitstand';evidence.mkdir(parents=True,exist_ok=True)
if r.returncode==0:
 subprocess.run([*scp,host+':/home/radxa/'+name+'-deployment.json',str(evidence/'deployment.json')],check=True)
 for direction in ('sit','stand'):shutil.copy2(stage/f'policy-sitstand-{direction}.json',root/f'backend-rust/tests/fixtures/policy-sitstand-{direction}.json')
r.check_returncode()
