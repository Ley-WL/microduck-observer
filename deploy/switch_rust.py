"""Atomic release/service switch with automatic rollback; never send motion."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
from urllib.request import urlopen

parser = argparse.ArgumentParser()
parser.add_argument('--release', type=Path, required=True)
parser.add_argument('--backup', type=Path, required=True)
args = parser.parse_args()
if os.geteuid() != 0:
    raise SystemExit('Administrator required for systemd switch')
release = args.release.resolve()
releases = Path('/home/radxa/microduck-observer/releases').resolve()
if release.parent != releases:
    raise SystemExit('Release must be directly inside the managed releases directory')
current = releases.parent/'current'
service = Path('/etc/systemd/system/microduck-observer.service')
calibration = Path('/var/lib/microduck-observer/calibration.json')
backup = args.backup.resolve()
backup.mkdir(parents=True, exist_ok=True)


def request(path):
    with urlopen('http://127.0.0.1:8877/api/v1/'+path, timeout=3) as response:
        return json.load(response)


def manifest(root):
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in root.rglob('*') if p.is_file()}


def point(target):
    temporary = current.with_name('current.rust-staging')
    if temporary.is_symlink():
        temporary.unlink()
    elif temporary.exists():
        raise RuntimeError('Staging link path is occupied by a regular file')
    temporary.symlink_to(target, target_is_directory=True)
    os.replace(temporary, current)


previous = current.resolve()
before = request('snapshot')
control = before.get('system', {}).get('data', {}).get('servoControl') or {}
if control.get('state') in ('preflight', 'moving', 'policy', 'stopping'):
    raise SystemExit('Active motion task; leave production unchanged')
assert (release/'bin/microduck-observer').is_file()
assert manifest(release/'frontend/dist') == manifest(previous/'frontend/dist')
assert (release/'lib/libonnxruntime.so').is_file()
assert (backup/'candidate-verification.json').is_file()
assert json.loads((backup/'candidate-verification.json').read_text())['ok'] is True
shutil.copy2(service, backup/'service-before-switch')
shutil.copy2(calibration, backup/'calibration-before-switch.json')
os.chmod(backup/'calibration-before-switch.json', 0o600)
(backup/'release-before-switch.txt').write_text(str(previous))
try:
    subprocess.run(['systemctl', 'stop', 'microduck-observer'], check=True)
    point(release)
    shutil.copyfile(release/'deploy/microduck-observer-rust.service', service)
    os.chmod(service, 0o644)
    subprocess.run(['systemctl', 'daemon-reload'], check=True)
    subprocess.run(['systemctl', 'start', 'microduck-observer'], check=True)
    for _ in range(100):
        try:
            if request('health')['status'] == 'ok':
                break
        except OSError:
            pass
        time.sleep(.1)
    else:
        raise TimeoutError('Native service did not become healthy')
    after = request('snapshot')
    for key in ('joints', 'mounting'):
        assert after['calibration'][key] == before['calibration'][key]
    for key in ('quaternion', 'mountingQuaternion', 'targetQuaternion'):
        assert after['calibration']['imu'].get(key) == before['calibration']['imu'].get(key)
    assert len([r for r in after['joints']['data']['servos'] if r['online']]) == 15
    assert {r['id']: r['torque'] for r in after['joints']['data']['servos']} == {
        r['id']: r['torque'] for r in before['joints']['data']['servos']}
    pid = int(subprocess.check_output(['systemctl', 'show', 'microduck-observer', '--property=MainPID', '--value']))
    assert Path(f'/proc/{pid}/exe').resolve() == release/'bin/microduck-observer'
    report = {'ok': True, 'release': str(release), 'previousRelease': str(previous),
              'pid': pid, 'executable': str(Path(f'/proc/{pid}/exe').resolve()),
              'unchangedFrontendFiles': len(manifest(release/'frontend/dist')),
              'calibrationRevision': after['calibration']['revision'],
              'jointsOnline': 15, 'torqueUnchanged': True, 'motionStarted': False}
    (backup/'switch-result.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(report))
except BaseException:
    subprocess.run(['systemctl', 'stop', 'microduck-observer'])
    point(previous)
    shutil.copyfile(backup/'service-before-switch', service)
    subprocess.run(['systemctl', 'daemon-reload'], check=True)
    subprocess.run(['systemctl', 'start', 'microduck-observer'], check=True)
    print('Rolled back release/service; current calibration retained')
    raise
