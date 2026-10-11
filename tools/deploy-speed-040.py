"""Deploy 0.4 m/s command limits without changing models or calibration."""
import hashlib
import subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[1]
name='20261010-speed-040-v1'
stage=root/'deploy/staging/speed-040-20261010'
stage.mkdir(parents=True,exist_ok=True)
script=(root/'deploy/install-hd1910-skills.py').read_text()
script=script.replace('Install qualified replica/HD1910 recovery and sitstand models','Install policy trace service; preserve all models; update frontend speed controls')
script=script.replace("'20261010-hd1910-skills-v1'",repr(name))
script=script.replace('/home/radxa/hd1910-skills-stage','/home/radxa/speed-040-stage')
start=script.index('# Keep old release intact;')
end=script.index('# Extract reviewed frontend',start)
script=script[:start]+"manifest['modelsExpected']=tree(release/'models')\n"+script[end:]
script=script.replace('before = ready()',"gains=cal.with_name('model-gains.json')\ngains_before=gains.read_bytes() if gains.exists() else None\nbefore = ready()")
script=script.replace("    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'", "    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'\n    assert (gains.read_bytes() if gains.exists() else None)==gains_before, 'Model gains changed'\n    manifest['modelGainsUnchanged']=True")
installer=stage/'install.py';installer.write_text(script)
compile(script,str(installer),'exec')
host='radxa@8.133.210.70'
ssh=['ssh','-o','BatchMode=yes','-o','ConnectTimeout=8','-o','HostKeyAlias=192.168.31.193','-p','5722',host]
scp=['scp','-O','-o','BatchMode=yes','-o','ConnectTimeout=8','-o','HostKeyAlias=192.168.31.193','-P','5722']
remote='/home/radxa/speed-040-stage/'
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
binary=root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer'
script=script.replace("manifest['binarySha256'] = sha(release/'bin/microduck-observer')", "manifest['binarySha256'] = sha(release/'bin/microduck-observer')\nassert manifest['binarySha256']=="+repr(hashlib.sha256(binary.read_bytes()).hexdigest()))
installer.write_text(script)
import tarfile
archive=stage/'frontend.tar.gz'
with tarfile.open(archive,'w:gz') as tar:tar.add(root/'frontend/dist',arcname='dist')
files=[(installer,'install.py'),(binary,'observer.new'),(archive,'frontend.tar.gz')]
files += [((root/'deploy/staging/sit-smooth-20261010'/n if 'sitstand' in n else root/'backend-rust/tests/fixtures'/n),n) for n in ('policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json')]
for source,target in files:subprocess.run([*scp,str(source),host+':'+remote+target],check=True)
credentials=Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt')
password=next(line.removeprefix('Password: ') for line in credentials.read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run([*ssh,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python "+remote+'install.py'],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
evidence=root/'docs/实测记录/附件/平台/2026-10-10/speed-040'
evidence.mkdir(parents=True,exist_ok=True)
if result.returncode==0:subprocess.run([*scp,host+':/home/radxa/'+name+'-deployment.json',str(evidence/'deployment.json')],check=True)
result.check_returncode()
