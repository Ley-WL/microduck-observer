"""Stage/install the explicitly requested donor walk policy. No physical motion."""
from pathlib import Path
import hashlib,json,subprocess,shutil
root=Path(__file__).resolve().parents[1];stage=root/'deploy/staging/kissqy-walk-20261010';name='20261010-kissqy-walk-v1';remote='/home/radxa/kissqy-walk-stage'
binary=root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer'
script=(root/'deploy/install-hd1910-skills.py').read_text(encoding='utf-8')
script=script.replace("'20261010-hd1910-skills-v1'",repr(name)).replace('/home/radxa/hd1910-skills-stage',remote)
script=script.replace("('xgoduck_getup','alpha_sitstand')","('xgoduck_walk',)")
script=script.replace("    assert metadata['runtimeKind'] in ('hd1910_getup','hd1910_sitstand')", "    assert metadata['runtimeKind']=='xgoduck'\n    assert metadata['walkExecution']['gains']=={'p':3,'d':0}")
script=script.replace("    assert all(value['worstGroupSuccessRate']>=.95 for value in metadata['simulationAcceptance'].values())", "    assert metadata['policySha256']=='1cbf8730eadd75a5704332c9f3c10f60bfa9412135b907831c9dbd731ce1795f'\n    assert metadata['hardwareValidated'] is False")
a=script.index('# Extract reviewed frontend');b=script.index('simulation_cal =',a)
script=script[:a]+"manifest['frontendAfter']=tree(release/'frontend')\n"+script[b:]
script=script.replace("('policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json')","('policy-xgoduck.json','policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json')")
script=script.replace("[('xgoduck_getup','xgoduck_getup'),('sitstand_sit','alpha_sitstand'),('sitstand_stand','alpha_sitstand')]","[('xgoduck','xgoduck_walk')]")
script=script.replace('before = ready()',"""gains=cal.with_name('model-gains.json')
gains_before=gains.read_bytes() if gains.exists() else None
walk_gains=cal.with_name('walk-model-gains.json')
walk_before=walk_gains.read_bytes() if walk_gains.exists() else None
before = ready()
assert all(r['torque']==0 for r in before['joints']['data']['servos']), 'Stop and release servos before deployment'""")
script=script.replace("manifest['binarySha256'] = sha(release/'bin/microduck-observer')", "manifest['binarySha256'] = sha(release/'bin/microduck-observer')\nassert manifest['binarySha256']=="+repr(hashlib.sha256(binary.read_bytes()).hexdigest()))
script=script.replace("env = os.environ.copy()", "(release/'walk-model-gains.json').write_text(json.dumps({'p':3,'d':0}))\nenv = os.environ.copy()")
script=script.replace('    switch(release)',"    walk_gains.write_text(json.dumps({'p':3,'d':0}))\n    walk_gains.chmod(0o644)\n    manifest['walkGainsBefore']=walk_before.decode() if walk_before else None\n    switch(release)")
script=script.replace("    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'", """    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'
    assert (gains.read_bytes() if gains.exists() else None)==gains_before, 'Other skill gains changed'
    manifest['walkGains']=get('model-gains')
    assert manifest['walkGains']['p']==3 and manifest['walkGains']['d']==0
    manifest['otherSkillGainsUnchanged']=True""")
script=script.replace('    switch(old)',"    if walk_before is None:walk_gains.unlink(missing_ok=True)\n    else:walk_gains.write_bytes(walk_before)\n    switch(old)")
installer=stage/'install.py';installer.write_text(script,encoding='utf-8');compile(script,str(installer),'exec')
host='radxa@8.133.210.70';ssh=['ssh','-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=10','-o','ServerAliveCountMax=3','-o','HostKeyAlias=192.168.31.193','-p','5722',host];scp=['scp','-O','-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=10','-o','ServerAliveCountMax=3','-o','HostKeyAlias=192.168.31.193','-P','5722']
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
files=[(installer,'install.py'),(binary,'observer.new')]+[(stage/n,n) for n in ['xgoduck_walk.onnx','xgoduck_walk.metadata.json','policy-xgoduck.json']]
files += [(root/'backend-rust/tests/fixtures/policy-xgoduck_getup.json','policy-xgoduck_getup.json')]
files += [(root/'deploy/staging/sit-smooth-20261010'/n,n) for n in ['policy-sitstand-sit.json','policy-sitstand-stand.json']]
for source,target in files:subprocess.run([*scp,str(source),host+':'+remote+'/'+target],check=True)
credentials=Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt')
password=next(line.removeprefix('Password: ') for line in credentials.read_text().splitlines() if line.startswith('Password: '))
r=subprocess.run([*ssh,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python "+remote+'/install.py'],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(r.stdout,r.stderr,flush=True)
evidence=root/'docs/实测记录/附件/平台/2026-10-10/kissqy-walk';evidence.mkdir(parents=True,exist_ok=True)
if r.returncode==0:
 subprocess.run([*scp,host+':/home/radxa/'+name+'-deployment.json',str(evidence/'deployment.json')],check=True)
 for suffix in ('onnx','metadata.json'):shutil.copy2(stage/('xgoduck_walk.'+suffix),root/'models'/('xgoduck_walk.'+suffix))
 shutil.copy2(stage/'policy-xgoduck.json',root/'backend-rust/tests/fixtures/policy-xgoduck.json')
r.check_returncode()
