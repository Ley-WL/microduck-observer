"""Model-only release overlay. No actuator, calibration or network commands."""
import hashlib,json,os,shutil,subprocess,time,urllib.request
from pathlib import Path

stage=Path(__file__).resolve().parent
root=Path('/home/radxa/microduck-observer')
name='getup-mechanical-20261012-v2'
expected='4b9c82e5bbd003a465b9bf861d4e7020014e7781d311f3780001e5257a588838'
changed={'models/xgoduck_getup.onnx','models/xgoduck_getup.metadata.json','bin/microduck-observer'}
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def get(path):
    return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/'+path,timeout=5))
def idle(s):
    rows=s['joints']['data']['servos']
    return s['system']['data']['servoControl']['state'] not in ['policy','preflight','moving','stopping','disabling'] and len(rows)==15 and all(r['online'] and r['torque']==0 for r in rows)
def files(base):
    return {p.relative_to(base).as_posix():sha(p) for p in base.rglob('*') if p.is_file()}
def regs(s):
    keys=['torque','kpRaw','kiRaw','kdRaw','torqueLimitRaw','accelerationRaw','speedLimitRaw']
    return {str(r['id']):{k:r[k] for k in keys} for r in s['joints']['data']['servos']}
def point(target):
    p=root/'current.getup4750-stage'
    assert not p.exists() and not p.is_symlink()
    p.symlink_to(target);os.replace(p,root/'current')

before=get('snapshot');assert idle(before),'Robot active or enabled; deployment stopped'
(stage/'before.json').write_text(json.dumps(before,indent=2))
old=(root/'current').resolve();new=root/'releases'/name
assert not new.exists(),'Release already exists; inspect instead of repeating'
assert sha(stage/'xgoduck_getup.onnx')==expected
assert sha(stage/'microduck-observer')=='f41d2271ead378f903f4eff6bb92c9b03819b874b60aeda2ebdb0387a7926d7a'
meta=json.loads((stage/'xgoduck_getup.metadata.json').read_text())
assert meta['policySha256']==expected and meta['runtimeKind']=='hd1910_getup' and meta['actionEmaOldWeight']==.45
assert json.loads(Path('/var/lib/microduck-observer/model-gains.json').read_text())=={'p':5,'d':20}
old_files=files(old)
state_files={str(p):sha(p) for base in [Path('/var/lib/microduck-observer'),Path('/var/lib/ducklink')] for p in base.glob('*.json')}
shutil.copytree(old,new,symlinks=True)
for n in changed:shutil.copy2(stage/Path(n).name,new/n)
os.chmod(new/"bin/microduck-observer",0o755)
assert all(sha(new/k)==v for k,v in old_files.items() if k not in changed)
env=dict(os.environ,MICRODUCK_POLICY_PATH=str(new/'models/hd1910-head-v5.onnx'),
         MICRODUCK_CALIBRATION_FILE='/var/lib/microduck-observer/calibration.json',
         ORT_DYLIB_PATH=str(new/'lib/libonnxruntime.so'))
parity=subprocess.run([str(new/'bin/microduck-observer'),'--verify-policy-fixture',str(stage/'policy-getup4750.json')],env=env,cwd=new,capture_output=True,text=True,check=True)
(stage/'arm-parity.log').write_text(parity.stdout+parity.stderr)
arm=json.loads(parity.stdout.splitlines()[-1]);assert arm['ok'] and arm['policySha256']==expected and not arm['hardwareAccess']
assert arm['kpRun']==5 and arm['kdRun']==20
assert idle(get('snapshot')),'Robot changed state before switch; deployment stopped'
try:
    subprocess.run(['systemctl','stop','microduck-observer'],check=True)
    point(new)
    subprocess.run(['systemctl','start','microduck-observer'],check=True)
    for _ in range(80):
        try:
            after=get('snapshot')
            if idle(after):break
        except Exception:pass
        time.sleep(.15)
    else:raise RuntimeError('Service did not recover idle/unloaded')
    # API bootId identifies this service instance; it is not a saved calibration.
    saved=lambda s:{k:v for k,v in s['calibration'].items() if k!='bootId'}
    assert saved(before)==saved(after),'Calibration values changed'
    assert regs(before)==regs(after),'Servo profiles changed'
    assert all(sha(Path(k))==v for k,v in state_files.items()),'Persistent settings changed'
    assert all(sha(new/k)==v for k,v in old_files.items() if k not in changed)
    samples=[]
    for _ in range(20):
        samples.append(get('snapshot'));time.sleep(.1)
    assert all(idle(s) for s in samples)
    assert all(subprocess.run(['systemctl','is-enabled',n],capture_output=True,text=True).stdout.strip()=='masked' for n in ['tofd','rsyslog'])
    policy=get('policy');(stage/'policy-after.json').write_text(json.dumps(policy,indent=2))
    (stage/'after.json').write_text(json.dumps(after,indent=2))
    report={'installed':True,'release':str(new),'previousRelease':str(old),'policySha256':expected,
            'changedFiles':sorted(changed),'frontendOtherModelsUnchanged':True,
            'calibrationGainsIdentityAndServoProfilesUnchanged':True,'readOnlySnapshots':20,
            'hardwareMotion':False,'armParity':arm,'modelMetadataSha256':sha(new/'models/xgoduck_getup.metadata.json'),
            'binarySha256':sha(new/'bin/microduck-observer'),'actionEmaOldWeight':.45,
            'getupMinSeconds':3,'uprightHoldSeconds':1,'hardwareValidated':False,'formalAcceptancePendingAtDeployment':True,'userAuthorizedEarlyDeployment':True,'previousBinarySha256':old_files['bin/microduck-observer']}
    (stage/'deployment.json').write_text(json.dumps(report,indent=2));print(json.dumps(report))
except BaseException:
    subprocess.run(['systemctl','stop','microduck-observer']);point(old)
    subprocess.run(['systemctl','start','microduck-observer']);raise
