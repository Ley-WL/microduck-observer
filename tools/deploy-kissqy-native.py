"""Install native donor execution, current Web build, and donor roulade only.

The installer refuses active control or enabled servos and performs no motion.
"""
from pathlib import Path
import hashlib,json,re,subprocess,tarfile,shlex,time

root=Path(__file__).resolve().parents[1]
name='20261011-kissqy-native-v2';remote='/home/radxa/kissqy-native-stage'
stage=root/'deploy/staging/kissqy-native-20261011';stage.mkdir(parents=True,exist_ok=True)
evidence=root/'docs/实测记录/附件/平台/2026-10-11/kissqy-native'
audit=json.loads((evidence/'audit.json').read_text(encoding='utf-8'))
assert all(m['matched'] for m in audit['models'].values())
binary=root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
with tarfile.open(stage/'frontend.tar.gz','w:gz') as archive:
    archive.add(root/'frontend/dist',arcname='dist')
frontend_hashes={p.relative_to(root/'frontend/dist').as_posix():sha(p) for p in (root/'frontend/dist').rglob('*') if p.is_file()}
(stage/'frontend-hashes.json').write_text(json.dumps(frontend_hashes),encoding='utf-8')
text=(root/'deploy/staging/kissqy-mechanical-20261011/install.py').read_text(encoding='utf-8')
text=text.replace('20261011-kissqy-mechanical-v1',name).replace('/home/radxa/kissqy-mechanical-stage',remote)
text=re.sub(r"assert manifest\['binarySha256'\]=='[a-f0-9]{64}'",f"assert manifest['binarySha256']=={sha(binary)!r}",text)
old="# Only the native binary changes. All installed model files remain identical.\nassert tree(release/'models')==manifest['modelsBefore']\n"
new="""# Keep all other current board artifacts and the old release for rollback.
stage=Path('/home/radxa/kissqy-native-stage')
replaced={'xgoduck_roulade.onnx','xgoduck_roulade.metadata.json'}
for filename in replaced:shutil.copy2(stage/filename,release/'models'/filename)
assert {k:v for k,v in tree(release/'models').items() if k not in replaced}=={k:v for k,v in manifest['modelsBefore'].items() if k not in replaced}
roll=json.loads((release/'models/xgoduck_roulade.metadata.json').read_text())
assert roll['runtimeKind']=='kissqy_roulade' and sha(release/'models/xgoduck_roulade.onnx')==roll['policySha256']
with tarfile.open(stage/'frontend.tar.gz') as archive:
    for member in archive.getmembers():
        assert (member.isfile() or member.isdir()) and (release/'frontend') in (release/'frontend'/member.name).resolve().parents
    assert dist.resolve().parent==release/'frontend'
    shutil.rmtree(dist)
    archive.extractall(release/'frontend')
assert tree(dist)==json.loads((stage/'frontend-hashes.json').read_text())
assert all(subprocess.run(['systemctl','is-enabled',service],capture_output=True,text=True).stdout.strip()=='masked' for service in ['tofd','rsyslog'])
"""
assert old in text;text=text.replace(old,new)
text=text.replace("'policy-xgoduck_pick.json','policy-xgoduck_getup.json'","'policy-xgoduck_pick.json','policy-xgoduck_roulade.json','policy-xgoduck_getup.json'")
text=text.replace("    manifest['policyInfo']=get('policy')", "    assert all(subprocess.run(['systemctl','is-enabled',service],capture_output=True,text=True).stdout.strip()=='masked' for service in ['tofd','rsyslog'])\n    manifest['policyInfo']=get('policy')")
installer=stage/'install.py';installer.write_text(text,encoding='utf-8');compile(text,str(installer),'exec')
options=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2','-o','HostKeyAlias=192.168.31.193']
host='radxa@8.133.210.70';ssh=['ssh',*options,'-p','5722',host];scp=['scp','-O',*options,'-P','5722']
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
files=[(installer,'install.py'),(binary,'observer.new'),(stage/'frontend.tar.gz','frontend.tar.gz'),(stage/'frontend-hashes.json','frontend-hashes.json'),
       (root/'backend-rust/tests/fixtures/kissqy-mechanical-limits.json','kissqy-mechanical-limits.json'),
       (root/'deploy/staging/kissqy-mechanical-20261011/policy-saved-getup-targets.json','policy-saved-getup-targets.json')]
files += [(root/'models'/f'xgoduck_roulade.{ext}',f'xgoduck_roulade.{ext}') for ext in ['onnx','metadata.json']]
files += [(root/'backend-rust/tests/fixtures'/('policy-'+kind+'.json'),'policy-'+kind+'.json') for kind in ['xgoduck','xgoduck_pick','xgoduck_roulade','xgoduck_getup','sitstand-sit','sitstand-stand']]
package=stage/'payload.tar.gz'
with tarfile.open(package,'w:gz') as archive:
    for source,target in files:archive.add(source,arcname=target)
for attempt in range(3):
    uploaded=subprocess.run([*scp,str(package),host+':'+remote+'/payload.tar.gz'])
    if uploaded.returncode==0:break
    if attempt==2:uploaded.check_returncode()
    time.sleep(3)
credentials=Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt')
password=next(line.removeprefix('Password: ') for line in credentials.read_text().splitlines() if line.startswith('Password: '))
remote_code=f"import tarfile,runpy; p={remote!r}; a=tarfile.open(p+'/payload.tar.gz'); assert all('/' not in m.name and m.isfile() for m in a.getmembers()); a.extractall(p); runpy.run_path(p+'/install.py',run_name='__main__')"
reply=subprocess.run([*ssh,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python -c "+shlex.quote(remote_code)],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(reply.stdout,reply.stderr,flush=True)
if reply.returncode==0:
    subprocess.run([*scp,host+':/home/radxa/'+name+'-deployment.json',str(evidence/'deployment.json')],check=True)
    paths=[root/'backend-rust/src'/n for n in ['policy.rs','control.rs','telemetry.rs','ducklink_motion.rs','walk_profile.rs','gains.rs']]+[installer,Path(__file__),root/'tools/audit-kissqy-native.py',binary]
    (evidence/'source-hashes.json').write_text(json.dumps({str(p.relative_to(root)):sha(p) for p in paths},indent=2),encoding='utf-8')
reply.check_returncode()
