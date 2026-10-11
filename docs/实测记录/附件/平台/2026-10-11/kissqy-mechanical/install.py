"""Install qualified replica/HD1910 recovery and sitstand models; no motion test."""
import math
import hashlib
import json
import os
import shutil
import subprocess
import time
import tarfile
import urllib.request
from pathlib import Path

root = Path('/home/radxa/microduck-observer')
current = root / 'current'
old = current.resolve()
deployment_name = os.environ.get('MICRODUCK_DEPLOYMENT_NAME', '20261011-kissqy-mechanical-v1')
assert deployment_name and all(c.isalnum() or c == '-' for c in deployment_name)
release = root / 'releases' / deployment_name
cal = Path('/var/lib/microduck-observer/calibration.json')
ids = {10,11,12,13,14,20,21,22,23,24,30,31,32,33,34}
keys = ('torque','goalPositionRaw','goalCurrentRaw','accelerationRaw','speedLimitRaw','torqueLimitRaw','kpRaw','kiRaw','kdRaw')

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def physical_calibration(value):
    value = json.loads(json.dumps(value))
    value.pop('revision', None)
    value.pop('updatedAt', None)
    value.get('imu', {}).pop('bootId', None)
    return value

def tree(path):
    return {str(f.relative_to(path)):sha(f) for f in path.rglob('*') if f.is_file()}

def get(endpoint, port=8877):
    return json.load(urllib.request.urlopen(f'http://127.0.0.1:{port}/api/v1/{endpoint}', timeout=3))

def ready():
    for _ in range(400):
        try:
            snap = get('snapshot')
            state = snap['system']['data']['servoControl']['state']
            assert state not in ('policy','moving','preflight','disabling'), 'active control'
            rows = snap['joints']['data']['servos']
            if {r['id'] for r in rows} == ids and all(r['online'] and r['torque'] in (0,1) and r['profileFresh'] for r in rows):
                return snap
        except (OSError, KeyError):
            pass
        time.sleep(.025)
    raise RuntimeError('No complete fresh profile snapshot; no motion test performed')

def registers(snap):
    return {str(r['id']):{k:r[k] for k in keys} for r in snap['joints']['data']['servos']}

def switch(path):
    temporary = root / 'current.hd1910-skills-next'
    assert not temporary.exists() and not temporary.is_symlink()
    temporary.symlink_to(path)
    os.replace(temporary, current)

assert current.is_symlink() and not release.exists()
before = ready()
assert all(r['torque']==0 for r in before['joints']['data']['servos']), 'Stop and release servos before deployment'
gains_before={n:(cal.parent/n).read_bytes() for n in ('model-gains.json','walk-model-gains.json')}
manifest = {'previous':str(old),'release':str(release),'calibrationBefore':sha(cal),'calibrationValuesBefore':json.loads(cal.read_text()),
            'modelsBefore':tree(old/'models'),'frontendBefore':tree(old/'frontend'),
            'servoRegistersBefore':registers(before)}
shutil.copytree(old, release, symlinks=True)
shutil.copy2('/home/radxa/kissqy-mechanical-stage/observer.new', release/'bin/microduck-observer')
(release/'bin/microduck-observer').chmod(0o755)
dist = release/'frontend/dist'
assert (dist/'index.html').is_file()
assert tree(release/'models') == manifest['modelsBefore']
manifest['binarySha256'] = sha(release/'bin/microduck-observer')
assert manifest['binarySha256']=='efc9ae1041fc7bc87c18645dac5754d1ed07e1dc42e9d2e0f8ea2d5eb71b50f5'
# Only the native binary changes. All installed model files remain identical.
assert tree(release/'models')==manifest['modelsBefore']
manifest['modelsExpected']=tree(release/'models')
manifest['frontendAfter']=tree(release/'frontend')
simulation_cal = release/'simulation-calibration.json'
shutil.copy2(cal, simulation_cal)
for n,v in gains_before.items():(release/n).write_bytes(v)
env = os.environ.copy()
env.update(MICRODUCK_BLE_ENABLED='0', MICRODUCK_SOURCE='simulation', MICRODUCK_BIND='127.0.0.1:8878',
           MICRODUCK_SERVO_PORT='', MICRODUCK_TOF_SOCKET='',
           MICRODUCK_STATIC_DIR=str(dist), MICRODUCK_CALIBRATION_FILE=str(simulation_cal),
           MICRODUCK_POLICY_PATH=str(release/'models/hd1910-head-v5.onnx'),
           ORT_DYLIB_PATH=str(release/'lib/libonnxruntime.so'))
with (release/'simulation-check.log').open('w') as log:
    process = subprocess.Popen([str(release/'bin/microduck-observer')], env=env, stdout=log, stderr=log)
    try:
        for _ in range(40):
            if process.poll() is not None:
                raise RuntimeError('Simulation candidate exited')
            try:
                manifest['simulationHealth'] = get('health',8878)
                manifest['simulationSnapshot'] = get('snapshot',8878)
                manifest['simulationModels']=get('policy',8878)
                assert all(row['available'] for row in manifest['simulationModels']['models'])
                candidate_limits=get('servos/limits',8878)['limits']
                mechanical=json.loads(Path('/home/radxa/kissqy-mechanical-stage/kissqy-mechanical-limits.json').read_text())
                for joint in mechanical['joints']:
                    actual=candidate_limits[str(joint['id'])]
                    expected=[math.degrees(joint[k]) for k in ('min_rad','max_rad')]
                    assert all(abs(a-b)<1e-8 for a,b in zip(actual,expected)), (joint['name'],actual,expected)
                manifest['targetEncoding']=get('servos/limits',8878)['targetEncoding']
                assert manifest['targetEncoding']=='mechanical-clamp-single-turn-reject-v1'
                manifest['candidateLimits']=candidate_limits
                html = urllib.request.urlopen('http://127.0.0.1:8878/',timeout=3).read()
                assert html == (dist/'index.html').read_bytes()
                break
            except OSError:
                time.sleep(.1)
        else:
            raise RuntimeError('Simulation candidate not ready')
    finally:
        process.terminate()
        process.wait(timeout=5)
        simulation_cal.unlink(missing_ok=True)

env['ORT_DYLIB_PATH']=str(release/'lib/libonnxruntime.so')
env['MICRODUCK_POLICY_PATH']=str(release/'models/hd1910-head-v5.onnx')
manifest['policyParity']={}
for fixture in ('policy-xgoduck.json','policy-xgoduck_pick.json','policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json','policy-saved-getup-targets.json'):
    manifest['policyParity'][fixture]=json.loads(subprocess.run([str(release/'bin/microduck-observer'),'--verify-policy-fixture',str(Path('/home/radxa/kissqy-mechanical-stage')/fixture)],env=env,check=True,capture_output=True,text=True).stdout)
assert all(r['mechanicalTargetsVerified'] and not r['hardwareAccess'] for r in manifest['policyParity'].values())
assert manifest['policyParity']['policy-saved-getup-targets.json']['savedTargetCasesVerified']==152
identity=Path('/var/lib/ducklink/identity.json')
manifest['identityBefore']=sha(identity)
# Check again immediately before stopping: no active movement and no state changes.
assert registers(ready()) == manifest['servoRegistersBefore']
assert sha(cal) == manifest['calibrationBefore']
subprocess.run(['systemctl','stop','microduck-observer'],check=True)
try:
    switch(release)
    subprocess.run(['systemctl','start','microduck-observer'],check=True)
    after = ready()
    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'
    assert all((cal.parent/n).read_bytes()==v for n,v in gains_before.items()), 'Persistent gains changed'
    manifest['gainsUnchanged']=True
    assert tree(release/'frontend')==manifest['frontendAfter']
    assert tree(release/'models')==manifest['modelsExpected']
    assert physical_calibration(json.loads(cal.read_text())) == physical_calibration(manifest['calibrationValuesBefore']), 'Physical calibration changed'
    assert registers(after) == manifest['servoRegistersBefore'], 'Servo registers changed'
    manifest.update(result='installed',calibrationAfter=sha(cal),calibrationValuesAfter=json.loads(cal.read_text()),servoRegistersAfter=registers(after),
                    health=get('health'),snapshot=after,modelsAfter=tree(release/'models'))
    manifest['limitsAfter']=get('servos/limits')
    assert manifest['limitsAfter']['limits']==manifest['candidateLimits']
    manifest['policyInfo']=get('policy')
    assert all(row['available'] for row in manifest['policyInfo']['models'])
    for kind, name in [('xgoduck_getup','xgoduck_getup'),('sitstand_sit','alpha_sitstand'),('sitstand_stand','alpha_sitstand')]:
        row=next(r for r in manifest['policyInfo']['models'] if r['kind']==kind)
        assert row['metadata']['policySha256']==sha(release/'models'/(name+'.onnx'))
    snapshots = []
    for _ in range(20):
        snapshot = get('snapshot')
        rows = snapshot['joints']['data']['servos']
        snapshots.append({'seq':snapshot['joints'].get('seq'),
                          'missing':[r['id'] for r in rows if not r['online']]})
        time.sleep(.05)
    manifest['readonlySnapshots'] = snapshots
except Exception as error:
    manifest.update(result='rolled-back',error=str(error))
    subprocess.run(['systemctl','stop','microduck-observer'],check=True)
    switch(old)
    subprocess.run(['systemctl','start','microduck-observer'],check=True)
    raise
finally:
    Path('/home/radxa/' + deployment_name + '-deployment.json').write_text(json.dumps(manifest))
print(json.dumps({'result':manifest['result'],'release':str(current.resolve()),
                  'physicalCalibrationUnchanged':physical_calibration(manifest['calibrationValuesAfter'])==physical_calibration(manifest['calibrationValuesBefore']),
                  'registersUnchanged':manifest['servoRegistersAfter']==manifest['servoRegistersBefore'],
                  'missingSnapshots':sum(bool(s['missing']) for s in manifest['readonlySnapshots'])}))
