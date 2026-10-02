"""Admin-only isolated read test; always restore production in finally.

No enable/angle/stand/policy-start/EEPROM calls. Candidate binds loopback only.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import shutil
import signal
import subprocess
import time
from urllib.request import urlopen

parser = argparse.ArgumentParser()
parser.add_argument('--release', type=Path, required=True)
parser.add_argument('--workspace', type=Path, required=True)
args = parser.parse_args()
if os.geteuid() != 0:
    raise SystemExit('Run as administrator to pause/restore the production service')
release = args.release.resolve()
workspace = args.workspace.resolve()
binary = release/'bin/microduck-observer'
calibration = Path('/var/lib/microduck-observer/calibration.json')
python = '/home/radxa/microduck-observer/venv/bin/python'


def get(port, path):
    with urlopen(f'http://127.0.0.1:{port}/api/v1/{path}', timeout=3) as response:
        return json.load(response)


def wait(port, process=None):
    for _ in range(100):
        if process is not None and process.poll() is not None:
            raise RuntimeError('Candidate exited before verification')
        try:
            snapshot = get(port, 'snapshot')
            if 'imu.raw' in snapshot and 'joints' in snapshot:
                return snapshot
        except OSError:
            pass
        time.sleep(.1)
    if port == 8878:
        diagnostics = {}
        for name in ('snapshot', 'health', 'logs'):
            try:
                diagnostics[name] = get(port, name)
            except OSError as exc:
                diagnostics[name] = str(exc)
        (workspace/'candidate-startup-failure.json').write_text(json.dumps(diagnostics, indent=2))
    raise TimeoutError('Service did not produce required samples')


before = get(8877, 'snapshot')
state = before.get('system', {}).get('data', {}).get('servoControl') or {}
if state.get('state') in ('preflight', 'moving', 'policy', 'stopping'):
    raise SystemExit('Production has an active motion task; leave it running')
workspace.mkdir(parents=True, exist_ok=True)
clone = workspace/'candidate-calibration.json'
shutil.copy2(calibration, clone)
account = pwd.getpwnam('radxa')
os.chown(clone, account.pw_uid, account.pw_gid)
os.chmod(clone, 0o600)
checksum = hashlib.sha256(clone.read_bytes()).hexdigest()
env = dict(os.environ, MICRODUCK_SOURCE='hardware', MICRODUCK_BIND='127.0.0.1:8878',
           MICRODUCK_IMU_DRIVER='ms901m', MICRODUCK_IMU_PORT='/dev/ttyS9',
           MICRODUCK_SERVO_PORT='/dev/ttyS2',
           MICRODUCK_SERVO_IDS='10,11,12,13,14,20,21,22,23,24,30,31,32,33,34',
           MICRODUCK_CALIBRATION_FILE=str(clone), MICRODUCK_STATIC_DIR=str(release/'frontend/dist'),
           MICRODUCK_POLICY_PATH=str(release/'models/hd1910-head-v5.onnx'),
           ORT_DYLIB_PATH=str(release/'lib/libonnxruntime.so'))
candidate = None
log = open(workspace/'candidate.log', 'w')
try:
    subprocess.run(['systemctl', 'stop', 'microduck-observer'], check=True)
    candidate = subprocess.Popen(['runuser', '-u', 'radxa', '-g', 'radxa',
                                  '-G', 'dialout', '-G', 'robot', '-G', 'i2c', '--', str(binary)],
                                 cwd=release, env=env, stdout=log, stderr=log,
                                 start_new_session=True)
    wait(8878, candidate)
    subprocess.run([python, str(workspace/'measure_runtime.py'), '--url', 'http://127.0.0.1:8878',
                    '--pid', str(candidate.pid), '--seconds', '20', '--output', str(workspace/'rust-candidate.json')], check=True)
    result = json.loads((workspace/'rust-candidate.json').read_text())
    original_torque = {str(r['id']): r['torque'] for r in before['joints']['data']['servos'] if r['online']}
    assert result['torqueBefore'] == original_torque
    assert result['torqueBefore'] == result['torqueAfter']
    assert len(result['torqueAfter']) == 15
    assert result['receivedHz']['joints'] > 40
    assert result['receivedHz']['imu.raw'] > 40
    assert result['completeSampledJointFrames']/result['sampledJointFrames'] >= .99
    assert hashlib.sha256(clone.read_bytes()).hexdigest() == checksum
    native = get(8878, 'snapshot')['calibration']
    for section in ('joints', 'mounting'):
        assert native[section] == before['calibration'][section]
    for key in ('quaternion', 'mountingQuaternion', 'targetQuaternion'):
        assert native['imu'].get(key) == before['calibration']['imu'].get(key)
    report = {'ok': True, 'candidateCalibrationSha256': checksum, 'hardwareWrites': False,
              'frontendReused': True, 'benchmark': result}
    (workspace/'candidate-verification.json').write_text(json.dumps(report, indent=2))
    print('PASS: hardware read-only, unchanged torque/reference, >40Hz WS, >=99% complete sampled frames')
finally:
    if candidate is not None and candidate.poll() is None:
        os.killpg(candidate.pid, signal.SIGTERM)
        try:
            candidate.wait(7)
        except subprocess.TimeoutExpired:
            os.killpg(candidate.pid, signal.SIGKILL)
            candidate.wait()
    log.close()
    subprocess.run(['systemctl', 'start', 'microduck-observer'], check=True)
    wait(8877)
    print('Production Python service restored')
