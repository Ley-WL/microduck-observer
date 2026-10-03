"""Deploy tested native control fix without replacing calibration."""
import json, os, shutil, subprocess, time, urllib.request
from pathlib import Path

root = Path('/home/radxa/microduck-observer')
current = root/'current'
old = current.resolve()
new = root/'releases/20261002-servo-write-order-fix'
assert old.name == '20261002-servo-nudge-fix'
assert not new.exists()
def get(route):
    with urllib.request.urlopen('http://127.0.0.1:8877'+route, timeout=5) as f:
        return json.load(f)
before = get('/api/v1/snapshot')
assert before['system']['data']['servoControl']['state'] not in ['preflight', 'moving', 'policy', 'disabling']
cal = Path('/var/lib/microduck-observer/calibration.json')
backup = root/'backups/servo-write-order-20261002'
backup.mkdir(parents=True, exist_ok=True)
shutil.copy2(cal, backup/'calibration-before.json')
(backup/'previous-release.txt').write_text(str(old))
shutil.copytree(old, new, symlinks=True)
shutil.copy2('/home/radxa/observer-write-order-fixed', new/'bin/microduck-observer')
(new/'bin/microduck-observer').chmod(0o755)
def point(path):
    tmp = root/'current.write-order'
    tmp.symlink_to(path)
    os.replace(tmp, current)
subprocess.run(['systemctl', 'stop', 'microduck-observer'], check=True)
try:
    point(new)
    subprocess.run(['systemctl', 'start', 'microduck-observer'], check=True)
    for _ in range(30):
        try:
            after = get('/api/v1/snapshot')
            if all(r.get('online') for r in after['joints']['data']['servos']):
                break
        except Exception:
            pass
        time.sleep(.2)
    else:
        raise RuntimeError('No complete joint feedback after deployment')
    before_torque = {r['id']: r['torque'] for r in before['joints']['data']['servos']}
    after_torque = {r['id']: r['torque'] for r in after['joints']['data']['servos']}
    assert before_torque == after_torque
    saved = json.loads((backup/'calibration-before.json').read_text())
    latest = json.loads(cal.read_text())
    assert saved['joints'] == latest['joints']
    assert saved['mounting'] == latest['mounting']
    assert saved['imu']['quaternion'] == latest['imu']['quaternion']
    (backup/'deployment.json').write_text(json.dumps({'old': str(old), 'new': str(new), 'torque': after_torque, 'calibrationPreserved': True}))
    print('Deployed', new, 'all online; torque and calibration preserved')
except BaseException:
    subprocess.run(['systemctl', 'stop', 'microduck-observer'], check=True)
    point(old)
    subprocess.run(['systemctl', 'start', 'microduck-observer'], check=True)
    raise
