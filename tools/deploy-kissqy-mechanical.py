"""Deploy mechanical-limit execution only; preserve every board model and setting."""
from pathlib import Path
import hashlib
import json
import math
import subprocess

root=Path(__file__).resolve().parents[1]
name='20261011-kissqy-mechanical-v1'
stage=root/'deploy/staging/kissqy-mechanical-20261011'
stage.mkdir(parents=True,exist_ok=True)
remote='/home/radxa/kissqy-mechanical-stage'
binary=root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
# Independent upstream formula against all 152 saved real frames, no hardware.
mechanical=json.loads((root/'backend-rust/tests/fixtures/kissqy-mechanical-limits.json').read_text())
bounds={j['id']:(j['min_rad'],j['max_rad']) for j in mechanical['joints']}
capture=root/'docs/实测记录/附件/平台/2026-10-11/getup-compare/policy-session-latest.json'
trace=json.loads(capture.read_text(encoding='utf-8'))
fixture=json.loads((root/'backend-rust/tests/fixtures/policy-xgoduck_getup.json').read_text(encoding='utf-8'))
fixture['targetCaseCalibration']=trace['calibration'];fixture['targetCases']=[]
cal=trace['calibration']['joints']
for frame in trace['frames']:
    if frame['type']!='inference':continue
    angles=[math.radians(v) for v in frame['preClipTargetDegrees']]
    goals={}
    for id,angle in zip(trace['jointIds'],angles):
        lo,hi=bounds[id];bounded=max(lo,min(hi,angle));key=str(id)
        raw=cal['references'][key]+bounded*4096/math.tau*cal['directions'].get(key,-1)
        goal=math.floor(raw+.5) if raw>=0 else math.ceil(raw-.5)
        assert 0<=goal<=4095
        goals[key]=goal
    fixture['targetCases'].append({'targetsRadians':angles,'expectedGoals':goals})
assert len(fixture['targetCases'])==152
saved_fixture=stage/'policy-saved-getup-targets.json'
saved_fixture.write_text(json.dumps(fixture),encoding='utf-8')
text=(root/'deploy/staging/kissqy-sitstand-20261010/install.py').read_text(encoding='utf-8')
text=text.replace('20261010-kissqy-sitstand-v1',name).replace('/home/radxa/kissqy-sitstand-stage',remote)
text=text.replace("assert manifest['binarySha256']=='d6817451ff65deee458fa4378a64981b70414e212c163c299212c267141006d0'",f"assert manifest['binarySha256']=={sha(binary)!r}")
start=text.index('# Keep old release intact;')
end=text.index("manifest['modelsExpected']=",start)
text=text[:start]+"# Only the native binary changes. All installed model files remain identical.\nassert tree(release/'models')==manifest['modelsBefore']\n"+text[end:]
start=text.index("                model_json=json.loads")
end=text.index("                manifest['candidateLimits']",start)
text=text[:start]+'''                mechanical=json.loads(Path('/home/radxa/kissqy-mechanical-stage/kissqy-mechanical-limits.json').read_text())
                for joint in mechanical['joints']:
                    actual=candidate_limits[str(joint['id'])]
                    expected=[math.degrees(joint[k]) for k in ('min_rad','max_rad')]
                    assert all(abs(a-b)<1e-8 for a,b in zip(actual,expected)), (joint['name'],actual,expected)
                manifest['targetEncoding']=get('servos/limits',8878)['targetEncoding']
                assert manifest['targetEncoding']=='mechanical-clamp-single-turn-reject-v1'
'''+text[end:]
text=text.replace("('policy-xgoduck.json','policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json')", "('policy-xgoduck.json','policy-xgoduck_pick.json','policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json','policy-saved-getup-targets.json')")
text=text.replace("identity=Path('/var/lib/ducklink/identity.json')", "assert all(r['mechanicalTargetsVerified'] and not r['hardwareAccess'] for r in manifest['policyParity'].values())\nassert manifest['policyParity']['policy-saved-getup-targets.json']['savedTargetCasesVerified']==152\nidentity=Path('/var/lib/ducklink/identity.json')")
text=text.replace("    manifest['policyInfo']=get('policy')", "    manifest['limitsAfter']=get('servos/limits')\n    assert manifest['limitsAfter']['limits']==manifest['candidateLimits']\n    manifest['policyInfo']=get('policy')")
installer=stage/'install.py';installer.write_text(text,encoding='utf-8');compile(text,str(installer),'exec')
options=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2','-o','HostKeyAlias=192.168.31.193']
host='radxa@8.133.210.70';ssh=['ssh',*options,'-p','5722',host];scp=['scp','-O',*options,'-P','5722']
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
files=[(installer,'install.py'),(binary,'observer.new'),(root/'backend-rust/tests/fixtures/kissqy-mechanical-limits.json','kissqy-mechanical-limits.json'),(saved_fixture,saved_fixture.name)]
files += [(root/'backend-rust/tests/fixtures'/('policy-'+kind+'.json'),'policy-'+kind+'.json') for kind in ['xgoduck','xgoduck_pick','xgoduck_getup','sitstand-sit','sitstand-stand']]
for source,target in files:
    subprocess.run([*scp,str(source),host+':'+remote+'/'+target],check=True)
credentials=Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt')
password=next(line.removeprefix('Password: ') for line in credentials.read_text().splitlines() if line.startswith('Password: '))
r=subprocess.run([*ssh,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python "+remote+'/install.py'],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(r.stdout,r.stderr,flush=True)
evidence=root/'docs/实测记录/附件/平台/2026-10-11/kissqy-mechanical'
evidence.mkdir(parents=True,exist_ok=True)
if r.returncode==0:
    subprocess.run([*scp,host+':/home/radxa/'+name+'-deployment.json',str(evidence/'deployment.json')],check=True)
    paths=[root/'backend-rust/src'/n for n in ['policy.rs','control.rs','telemetry.rs','main.rs']]+[installer,Path(__file__),binary]
    (evidence/'source-hashes.json').write_text(json.dumps({str(p.relative_to(root)):sha(p) for p in paths},indent=2),encoding='utf-8')
r.check_returncode()
