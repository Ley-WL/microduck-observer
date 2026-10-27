"""Install a versioned model/runtime release with idle gates and rollback."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import urllib.request

base = Path('/home/radxa/microduck-observer')
stage = Path('/home/radxa/getup-staged65-stage')
old = (base / 'current').resolve()
new = base / 'releases/runtime-20261027-getup-staged1500'
expected_sha = 'e3b90ac0e5359f91e14306b7e2143e0d5a1f4cd49cb256bca3fd71835250d895'

def get(path: str) -> dict:
    return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/' + path, timeout=8))

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def safe() -> dict:
    snapshot = get('snapshot')
    assert snapshot['system']['data']['servoControl']['state'] in ('idle', 'disabled', 'failed'), 'Active controller'
    rows = snapshot['joints']['data']['servos']
    assert len(rows) == 15 and all(row['online'] and row['torque'] == 0 for row in rows), 'Servos not confirmed disabled'
    assert snapshot['joints']['ageMs'] < 1000, 'Stale feedback'
    return snapshot

def stable_calibration(path: Path) -> dict:
    for attempt in range(10):
        try:
            value = json.loads(path.read_text())
            break
        except json.JSONDecodeError:
            if attempt == 9: raise
            time.sleep(0.1)
    for key in ['revision', 'updatedAt', 'bootId']:
        value.pop(key, None)
    value.get('imu', {}).pop('bootId', None)
    return value

def point(path: Path) -> None:
    temporary = base / 'current.flamingo-tmp'
    assert not temporary.exists() and not temporary.is_symlink()
    temporary.symlink_to(path)
    os.replace(temporary, base / 'current')

before = safe()
assert old.name == 'runtime-20261026-164257', str(old)
assert not new.exists(), 'Never overwrite an existing release'
gains = get('model-gains')
gamepad = get('gamepad/mode')
camera_before = get('camera')
protected = {str(p): sha(p) for directory in [Path('/var/lib/microduck-observer'), Path('/var/lib/ducklink')]
             for p in directory.glob('*.json') if not p.name.startswith('policy-')}
calibrations = {p: stable_calibration(Path(p)) for p in protected if Path(p).name in ['calibration.json', 'calibration.previous.json']}
old_files = {str(p.relative_to(old)): sha(p) for p in old.rglob('*') if p.is_file()}
settings = {str(p): sha(p) for p in Path('/etc/systemd/system/microduck-observer.service.d').glob('*') if p.is_file()}
(stage / 'before.json').write_text(json.dumps(before, indent=2))
(stage / 'rollback-release.txt').write_text(str(old))
shutil.copytree(old, new, symlinks=True)
changed_files={'bin/microduck-observer','models/xgoduck_getup.onnx','models/xgoduck_getup.metadata.json'}
for source, target in [('microduck-observer','bin/microduck-observer')]+[(name,'models/'+name) for name in ['xgoduck_getup.onnx','xgoduck_getup.metadata.json']]:
    target_path = new / target
    assert target_path.resolve().is_relative_to(new)
    shutil.copy2(stage / source, target_path)
    assert sha(stage / source) == sha(target_path)
(new / 'bin/microduck-observer').chmod(0o755)
# Preserve the deployed frontend and native vision assets.
assert sha(new / 'models/xgoduck_walk.onnx') == expected_sha
assert sha(new / 'bin/microduck-observer') == (stage / 'binary.sha256').read_text().strip()
assert all(sha(new / p) == digest for p, digest in old_files.items() if p not in changed_files)
env = dict(os.environ, MICRODUCK_POLICY_PATH=str(new / 'models/xgoduck_walk.onnx'),
           ORT_DYLIB_PATH=str(new / 'lib/libonnxruntime.so'),
           MICRODUCK_GAINS_FILE='/var/lib/microduck-observer/model-gains.json')
reports = {}
assert sha(new/'models/xgoduck_getup.onnx')=='b9724f67d5443a2bdf675c73f3ba08a92219417da4ea438563b47a3645ff9e83'
for mode in ['xgoduck_getup']:
    result = subprocess.run([str(new / 'bin/microduck-observer'), '--verify-policy-fixture',
                             str(stage / ('policy-' + mode + '.json'))],
                            env=env, capture_output=True, text=True, timeout=60)
    (stage / ('fixture-' + mode + '.log')).write_text(result.stdout + result.stderr)
    assert result.returncode == 0, result.stdout + result.stderr
    reports[mode] = json.loads(result.stdout.splitlines()[-1])
result = subprocess.run([str(new / 'bin/microduck-observer'), '--verify-model-bank'],
                        env=env, capture_output=True, text=True, timeout=60)
(stage / 'model-bank.json').write_text(result.stdout)
assert result.returncode == 0, result.stderr
safe()
switched = False
try:
    point(new)
    switched = True
    subprocess.run(['systemctl', 'restart', 'microduck-observer'], check=True, timeout=40)
    last_error = None
    for _ in range(60):
        try:
            after = safe()
            assert after['system']['data']['service']['release'] == new.name
            break
        except Exception as error:
            last_error = str(error)
            time.sleep(0.5)
    else:
        raise RuntimeError('Service not ready: ' + str(last_error))
    policies = get('policy')['models']
    assert all(p['available'] for p in policies)
    assert get('model-gains') == gains
    assert get('gamepad/mode')['debug'] == gamepad['debug']
    changed = [p for p, digest in protected.items() if sha(Path(p)) != digest]
    assert all(p in calibrations and stable_calibration(Path(p)) == calibrations[p] for p in changed), changed
    assert all(sha(Path(p)) == digest for p, digest in settings.items())
    assert all(sha(old / p) == digest for p, digest in old_files.items())
    assert all(sha(new / p) == digest for p, digest in old_files.items() if p not in changed_files)
    for p in (new / 'frontend/dist').rglob('*'):
        if p.is_file():
            url = 'http://127.0.0.1:8877/' + p.relative_to(new / 'frontend/dist').as_posix()
            assert hashlib.sha256(urllib.request.urlopen(url, timeout=8).read()).hexdigest() == sha(p)
    for _ in range(20):
        after = safe()
        time.sleep(0.1)
    camera = get('camera')
    assert camera['enabled'] == camera_before['enabled']
    if camera_before['streaming']:
        assert camera['streaming'] and camera['encoder'] == camera_before['encoder']
    report = dict(release=str(new), rollback=str(old), modelSha256=expected_sha,
                  binarySha256=sha(new / 'bin/microduck-observer'), fixtures=reports,
                  snapshots=20, oldReleaseUnchanged=True, otherModelsUnchanged=True,
                  persistentSettingsPreserved=True, calibrationMetadataChanges=changed,
                  gains=gains, gamepad=gamepad, camera=camera, snapshot=after,
                  physicalMotionTested=False, getupModelSha256=sha(new/'models/xgoduck_getup.onnx'))
    (stage / 'deployment.json').write_text(json.dumps(report, indent=2))
    print(json.dumps({key: report[key] for key in ['release', 'rollback', 'modelSha256', 'fixtures', 'snapshots', 'physicalMotionTested']}))
except BaseException:
    if switched:
        point(old)
        subprocess.run(['systemctl', 'restart', 'microduck-observer'], check=True, timeout=40)
    raise
