"""Deploy a screened temporal actor candidate with rollback; never run motion."""
import argparse,hashlib,json,shutil,subprocess
from pathlib import Path
import numpy as np
import onnxruntime as ort

root=Path(__file__).resolve().parents[1]
artifacts=root.parent/'microduck-rl/artifacts/sit-smooth-repair-20261010'
name='20261010-sit-smooth-v1'
expected='85d7e1a48f97d29af2482ad92ecf0685837e520d01960f34642feed0c82fc167'
parser=argparse.ArgumentParser();parser.add_argument('--prepare-only',action='store_true');args=parser.parse_args()

proof=[]
for condition in ('evaluation','heldout','velocitylag1'):
    reports={tag:json.loads((artifacts/condition/tag/'summary.json').read_text(encoding='utf-8')) for tag in ('baseline','weight-1p0')}
    def outcomes(report):
        return {(g['scenario'],g['seed'],r['trial']):r['success'] for g in report['groups'] for r in g['results']}
    before,after=outcomes(reports['baseline']),outcomes(reports['weight-1p0'])
    assert set(before)==set(after) and before, 'Incomplete paired screening'
    assert all(not success or after[key] for key,success in before.items()), 'Candidate lost a successful baseline trial'
    assert reports['weight-1p0']['policySha256']==expected
    proof.append({'condition':condition,'baselineSuccesses':sum(before.values()),'candidateSuccesses':sum(after.values()),
                  'trials':len(before),'regressions':0,'formalAccepted':reports['weight-1p0']['accepted']})

stage=root/'deploy/staging/sit-smooth-20261010';stage.mkdir(parents=True,exist_ok=True)
model=artifacts/'fit/weight-1p0.onnx';assert hashlib.sha256(model.read_bytes()).hexdigest()==expected
meta=json.loads(model.with_suffix('.metadata.json').read_text(encoding='utf-8'))
meta['deploymentProvenance'].update(pairedScreening=proof,formalAcceptancePassed=False,
    note='Short supervised temporal repair; fixed-input physical replay is in-sample. Physical smoothness still requires testing.')
shutil.copy2(model,stage/'alpha_sitstand.onnx')
(stage/'alpha_sitstand.metadata.json').write_text(json.dumps(meta,indent=2)+'\n',encoding='utf-8')
session=ort.InferenceSession(str(model),providers=['CPUExecutionProvider'])
for kind in ('sit','stand'):
    fixture=json.loads((root/f'backend-rust/tests/fixtures/policy-sitstand-{kind}.json').read_text(encoding='utf-8'))
    obs=np.array(fixture['observation'],dtype=np.float32)[None]
    fixture['action']=session.run(None,{session.get_inputs()[0].name:obs})[0].reshape(-1).tolist()
    (stage/f'policy-sitstand-{kind}.json').write_text(json.dumps(fixture,indent=2)+'\n',encoding='utf-8')

script=(root/'deploy/install-hd1910-skills.py').read_text(encoding='utf-8')
script=script.replace('Install qualified replica/HD1910 recovery and sitstand models','Install screened temporal actor candidate; preserve service/frontend')
script=script.replace("'20261010-hd1910-skills-v1'",repr(name)).replace('/home/radxa/hd1910-skills-stage','/home/radxa/sit-smooth-stage')
script=script.replace("shutil.copy2('/home/radxa/sit-smooth-stage/observer.new', release/'bin/microduck-observer')", "assert sha(release/'bin/microduck-observer')=='126e149b2e7e70f6b6dd46d5f4c8b003b342d39cd9be9cb24683ba9ab52dc483', 'Unreviewed service'")
script=script.replace("('xgoduck_getup','alpha_sitstand')","('alpha_sitstand',)")
script=script.replace("    assert all(value['worstGroupSuccessRate']>=.95 for value in metadata['simulationAcceptance'].values())",
    "    assert metadata['policySha256']=="+repr(expected)+"\n    assert metadata['skillPhysicsRevision']=='sit-self-contact-bounded-target-v3'\n    assert metadata['hardwareValidated'] is False\n    assert metadata['deploymentProvenance']['formalAcceptancePassed'] is False")
start=script.index('# Extract reviewed frontend');end=script.index('simulation_cal =',start)
script=script[:start]+"manifest['frontendAfter']=tree(release/'frontend')\n"+script[end:]
script=script.replace("('policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json')","('policy-sitstand-sit.json','policy-sitstand-stand.json')")
script=script.replace("[('xgoduck_getup','xgoduck_getup'),('sitstand_sit','alpha_sitstand'),('sitstand_stand','alpha_sitstand')]","[('sitstand_sit','alpha_sitstand'),('sitstand_stand','alpha_sitstand')]")
script=script.replace('before = ready()',"gains=cal.with_name('model-gains.json')\ngains_before=gains.read_bytes() if gains.exists() else None\nbefore = ready()")
script=script.replace("    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'", "    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'\n    assert (gains.read_bytes() if gains.exists() else None)==gains_before, 'Model gains changed'\n    manifest['modelGainsUnchanged']=True")
installer=stage/'install.py';installer.write_text(script,encoding='utf-8');compile(script,str(installer),'exec')
print(json.dumps({'policySha256':expected,'pairedScreening':proof,'prepareOnly':args.prepare_only}),flush=True)
if args.prepare_only:raise SystemExit(0)

host='radxa@8.133.210.70'
ssh=['ssh','-o','BatchMode=yes','-o','ConnectTimeout=8','-o','HostKeyAlias=192.168.31.193','-p','5722',host]
scp=['scp','-O','-o','BatchMode=yes','-o','ConnectTimeout=8','-o','HostKeyAlias=192.168.31.193','-P','5722']
remote='/home/radxa/sit-smooth-stage/'
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
for file in (installer,stage/'alpha_sitstand.onnx',stage/'alpha_sitstand.metadata.json',stage/'policy-sitstand-sit.json',stage/'policy-sitstand-stand.json'):
    subprocess.run([*scp,str(file),host+':'+remote+file.name],check=True)
credentials=Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt')
password=next(line.removeprefix('Password: ') for line in credentials.read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run([*ssh,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python "+remote+'install.py'],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
evidence=root/'docs/实测记录/附件/平台/2026-10-10/sit-smooth';evidence.mkdir(parents=True,exist_ok=True)
if result.returncode==0:
    subprocess.run([*scp,host+':/home/radxa/'+name+'-deployment.json',str(evidence/'deployment.json')],check=True)
result.check_returncode()
backup=stage/'previous-model';backup.mkdir(exist_ok=True)
for suffix in ('onnx','metadata.json'):
    target=root/'models'/('alpha_sitstand.'+suffix)
    if not (backup/target.name).exists():shutil.copy2(target,backup/target.name)
    shutil.copy2(stage/target.name,target)
for kind in ('sit','stand'):
    shutil.copy2(stage/f'policy-sitstand-{kind}.json',root/f'backend-rust/tests/fixtures/policy-sitstand-{kind}.json')
