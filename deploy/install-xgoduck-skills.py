"""Install XgoDuck skill models with simulation and read-only rollback checks."""
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
release = root / 'releases/20261008-xgoduck-skills-v1'
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
    temporary = root / 'current.skills-next'
    assert not temporary.exists() and not temporary.is_symlink()
    temporary.symlink_to(path)
    os.replace(temporary, current)

assert current.is_symlink() and not release.exists()
before = ready()
manifest = {'previous':str(old),'release':str(release),'calibrationBefore':sha(cal),'calibrationValuesBefore':json.loads(cal.read_text()),
            'modelsBefore':tree(old/'models'),'frontendBefore':tree(old/'frontend'),
            'servoRegistersBefore':registers(before)}
shutil.copytree(old, release, symlinks=True)
shutil.copy2('/home/radxa/observer-skills.new', release/'bin/microduck-observer')
(release/'bin/microduck-observer').chmod(0o755)
frontend_stage=Path('/home/radxa/ducklink-skills-stage/frontend')
frontend_stage.mkdir()
with tarfile.open('/home/radxa/ducklink-skills-stage/frontend.tar.gz') as archive:
    archive.extractall(frontend_stage,filter='data')
if (old/'frontend/dist/downloads').is_dir():
    shutil.copytree(old/'frontend/dist/downloads',frontend_stage/'downloads',dirs_exist_ok=True)
assert (release/'frontend/dist').resolve().is_relative_to(release.resolve())
shutil.rmtree(release/'frontend/dist')
shutil.copytree(frontend_stage,release/'frontend/dist')
dist = release/'frontend/dist'
assert (dist/'index.html').is_file()
assert tree(release/'models') == manifest['modelsBefore']
for skill in ('getup','pick','roulade'):
    name='xgoduck_'+skill
    for suffix in ('onnx','metadata.json'):
        shutil.copy2('/home/radxa/ducklink-skills-stage/'+name+'.'+suffix,release/'models'/(name+'.'+suffix))
    metadata=json.loads((release/'models'/(name+'.metadata.json')).read_text())
    assert sha(release/'models'/(name+'.onnx'))==metadata['policySha256']
assert all(sha(release/'models'/name)==digest for name,digest in manifest['modelsBefore'].items())
parity_env=os.environ.copy()
parity_env.update(MICRODUCK_POLICY_PATH=str(release/'models/hd1910-head-v5.onnx'),
                  ORT_DYLIB_PATH=str(release/'lib/libonnxruntime.so'))
manifest['modelParity']={}
for skill in ('getup','pick','roulade'):
    raw=subprocess.check_output([str(release/'bin/microduck-observer'),'--verify-policy-fixture',
        '/home/radxa/ducklink-skills-stage/policy-xgoduck_'+skill+'.json'],env=parity_env,text=True)
    manifest['modelParity'][skill]=json.loads(raw)

manifest['binarySha256'] = sha(release/'bin/microduck-observer')
manifest['frontendAfter'] = tree(release/'frontend')
assert tree(dist)==tree(frontend_stage)
simulation_cal = release/'simulation-calibration.json'
shutil.copy2(cal, simulation_cal)
env = os.environ.copy()
env.update(MICRODUCK_SOURCE='simulation', MICRODUCK_BIND='127.0.0.1:8878',
           MICRODUCK_SERVO_PORT='', MICRODUCK_TOF_SOCKET='',
           MICRODUCK_STATIC_DIR=str(dist), MICRODUCK_CALIBRATION_FILE=str(simulation_cal))
with (release/'simulation-check.log').open('w') as log:
    process = subprocess.Popen([str(release/'bin/microduck-observer')], env=env, stdout=log, stderr=log)
    try:
        for _ in range(40):
            if process.poll() is not None:
                raise RuntimeError('Simulation candidate exited')
            try:
                manifest['simulationHealth'] = get('health',8878)
                manifest['simulationSnapshot'] = get('snapshot',8878)
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

# Check again immediately before stopping: no active movement and no state changes.
assert registers(ready()) == manifest['servoRegistersBefore']
assert sha(cal) == manifest['calibrationBefore']
subprocess.run(['systemctl','stop','microduck-observer'],check=True)
try:
    switch(release)
    subprocess.run(['systemctl','start','microduck-observer'],check=True)
    after = ready()
    assert physical_calibration(json.loads(cal.read_text())) == physical_calibration(manifest['calibrationValuesBefore']), 'Physical calibration changed'
    assert registers(after) == manifest['servoRegistersBefore'], 'Servo registers changed'
    manifest.update(result='installed',calibrationAfter=sha(cal),calibrationValuesAfter=json.loads(cal.read_text()),servoRegistersAfter=registers(after),
                    health=get('health'),snapshot=after,modelsAfter=tree(release/'models'))
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
    Path('/home/radxa/skills-deployment.json').write_text(json.dumps(manifest))
print(json.dumps({'result':manifest['result'],'release':str(current.resolve()),
                  'physicalCalibrationUnchanged':physical_calibration(manifest['calibrationValuesAfter'])==physical_calibration(manifest['calibrationValuesBefore']),
                  'registersUnchanged':manifest['servoRegistersAfter']==manifest['servoRegistersBefore'],
                  'missingSnapshots':sum(bool(s['missing']) for s in manifest['readonlySnapshots'])}))
