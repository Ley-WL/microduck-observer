"""Run as root on board after uploading a staged power-controls bundle."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import urllib.request

root = Path('/home/radxa/microduck-observer')
stage = Path('/home/radxa/power-controls-stage')
new = root / 'releases/20261003-power-controls'
old = (root / 'current').resolve()
backup = root / 'backups/20261003-power-controls'
cal = Path('/var/lib/microduck-observer/calibration.json')


def get(route):
    return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/' + route, timeout=5))


before = get('snapshot')
assert before['system']['data']['servoControl']['state'] not in ('preflight', 'moving', 'policy', 'disabling')
assert not new.exists(), 'Release already exists'
backup.mkdir(parents=True, exist_ok=False)
shutil.copy2(cal, backup / 'calibration.json')
saved = json.loads(cal.read_text())
(backup / 'previous-release.txt').write_text(str(old))
shutil.copytree(old, new, symlinks=True)
shutil.copy2(stage / 'microduck-observer', new / 'bin/microduck-observer')
(new / 'bin/microduck-observer').chmod(0o755)
shutil.rmtree(new / 'frontend/dist')
shutil.copytree(stage / 'dist', new / 'frontend/dist')
shutil.copy2(stage / 'power-helper.py', '/usr/local/lib/microduck-power-helper.py')
Path('/usr/local/lib/microduck-power-helper.py').chmod(0o644)
for name in ('microduck-power.socket', 'microduck-power@.service'):
    shutil.copy2(stage / name, Path('/etc/systemd/system') / name)
dropin = Path('/etc/systemd/system/microduck-observer.service.d/power.conf')
assert not dropin.exists()
dropin.write_text('[Service]\nEnvironment=MICRODUCK_POWER_CONTROL=1\n')
subprocess.run(['systemd-analyze', 'verify', '/etc/systemd/system/microduck-power.socket', '/etc/systemd/system/microduck-power@.service'], check=True)
subprocess.run(['systemctl', 'daemon-reload'], check=True)
subprocess.run(['systemctl', 'enable', '--now', 'microduck-power.socket'], check=True)


def point(path):
    tmp = root / 'current.power-controls'
    tmp.symlink_to(path)
    os.replace(tmp, root / 'current')


subprocess.run(['systemctl', 'stop', 'microduck-observer'], check=True)
try:
    point(new)
    subprocess.run(['systemctl', 'start', 'microduck-observer'], check=True)
    for _ in range(40):
        try:
            after = get('snapshot')
            break
        except Exception:
            time.sleep(.25)
    else:
        raise RuntimeError('Observer did not start')
    latest = json.loads(cal.read_text())
    for key in ('joints', 'mounting'):
        assert saved[key] == latest[key]
    for key in ('quaternion', 'mountingQuaternion', 'targetQuaternion'):
        assert saved['imu'].get(key) == latest['imu'].get(key)
    result = {'old': str(old), 'new': str(new), 'calibrationPreserved': True,
              'frontendSha256': hashlib.sha256((new / 'frontend/dist/index.html').read_bytes()).hexdigest(),
              'hardware': get('health')}
    (backup / 'deployment.json').write_text(json.dumps(result))
    print(json.dumps({k: v for k, v in result.items() if k != 'hardware'}))
except BaseException:
    subprocess.run(['systemctl', 'stop', 'microduck-observer'])
    point(old)
    dropin.unlink(missing_ok=True)
    subprocess.run(['systemctl', 'daemon-reload'])
    subprocess.run(['systemctl', 'start', 'microduck-observer'])
    raise
