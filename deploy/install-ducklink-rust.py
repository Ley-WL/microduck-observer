"""Migrate DuckLink BLE/control into Rust, retaining identity and read-only checks."""
import math
import hashlib
import json
import os
import shutil
import subprocess
import time
import urllib.request
from pathlib import Path

root = Path('/home/radxa/microduck-observer')
current = root / 'current'
old = current.resolve()
release = root / 'releases/20261008-ducklink-rust-v1'
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
    temporary = root / 'current.ducklink-rust-next'
    assert not temporary.exists() and not temporary.is_symlink()
    temporary.symlink_to(path)
    os.replace(temporary, current)

assert current.is_symlink() and not release.exists()
before = ready()
manifest = {'previous':str(old),'release':str(release),'calibrationBefore':sha(cal),'calibrationValuesBefore':json.loads(cal.read_text()),
            'modelsBefore':tree(old/'models'),'frontendBefore':tree(old/'frontend'),
            'servoRegistersBefore':registers(before)}
shutil.copytree(old, release, symlinks=True)
shutil.copy2('/home/radxa/observer-ducklink-rust.new', release/'bin/microduck-observer')
(release/'bin/microduck-observer').chmod(0o755)
dist = release/'frontend/dist'
assert (dist/'index.html').is_file()
assert tree(release/'models') == manifest['modelsBefore']
manifest['binarySha256'] = sha(release/'bin/microduck-observer')
manifest['frontendAfter'] = tree(release/'frontend')
assert manifest['frontendAfter'] == manifest['frontendBefore']
simulation_cal = release/'simulation-calibration.json'
shutil.copy2(cal, simulation_cal)
env = os.environ.copy()
env.update(MICRODUCK_BLE_ENABLED='0', MICRODUCK_SOURCE='simulation', MICRODUCK_BIND='127.0.0.1:8878',
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
                candidate_limits=get('servos/limits',8878)['limits']
                model_json=json.loads((dist/'model/model.json').read_text())
                for entry in model_json['bodies']:
                    joint=entry.get('joint')
                    if joint and joint.get('id')!=34:
                        expected=[math.degrees(v) for v in joint['range']]
                        actual=candidate_limits[str(joint['id'])]
                        assert all(abs(a-b)<1e-8 for a,b in zip(actual,expected)), (joint['name'],actual,expected)
                assert candidate_limits['34']==[0,30]
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

# Check again immediately before stopping: no active movement and no state changes.
assert registers(ready()) == manifest['servoRegistersBefore']
assert sha(cal) == manifest['calibrationBefore']
# Preserve the provisioned identity; only grant the unified process access.
import pwd
identity=Path('/var/lib/ducklink/identity.json');identity_dir=identity.parent
manifest['identityBefore']=sha(identity)
identity_stats={str(path):(path.stat().st_uid,path.stat().st_gid,path.stat().st_mode & 0o777) for path in (identity_dir,identity)}
paths=[Path('/etc/systemd/system/microduck-observer.service.d/ducklink-rust.conf'),
       Path('/etc/systemd/system/ducklink-wifi.service.d/observer-rust.conf'),
       Path('/etc/dbus-1/system.d/microduck-observer-ble.conf'),Path('/opt/ducklink-gateway/wifi_service.py')]
backups={path:path.read_bytes() if path.exists() else None for path in paths}
legacy_enabled=subprocess.run(['systemctl','is-enabled','ducklink-ble'],capture_output=True,text=True).stdout.strip()
legacy_active=subprocess.run(['systemctl','is-active','ducklink-ble'],capture_output=True,text=True).stdout.strip()
for path in paths:path.parent.mkdir(parents=True,exist_ok=True)
(release/'unified-service-backups').mkdir(mode=0o700)
for i,(path,data) in enumerate(backups.items()):
    if data is not None:(release/'unified-service-backups'/str(i)).write_bytes(data)
(release/'unified-service-backups/manifest.json').write_text(json.dumps({'paths':[str(p) for p in paths], 'identityStats':identity_stats,'legacyEnabled':legacy_enabled,'legacyActive':legacy_active}))
subprocess.run(['systemctl','stop','ducklink-ble'],check=True)
subprocess.run(['systemctl','stop','microduck-observer'],check=True)
try:
    paths[0].write_text('[Unit]\nAfter=bluetooth.service ducklink-wifi.service\nWants=bluetooth.service ducklink-wifi.service\n[Service]\nEnvironment=MICRODUCK_BLE_ENABLED=1\nEnvironment=MICRODUCK_BLE_IDENTITY=/var/lib/ducklink/identity.json\nSupplementaryGroups=bluetooth ducklink\nReadWritePaths=/var/lib/ducklink\n')
    paths[1].write_text('[Service]\nEnvironment=DUCKLINK_CONTROL_USER=radxa\n')
    paths[2].write_text('<busconfig><policy user="radxa"><allow send_destination="org.bluez"/><allow receive_sender="org.bluez"/></policy></busconfig>')
    shutil.copy2('/home/radxa/ducklink-wifi-unified.py',paths[3])
    for path in (identity_dir,identity):os.chown(path,pwd.getpwnam('radxa').pw_uid,pwd.getpwnam('ducklink').pw_gid)
    identity_dir.chmod(0o700);identity.chmod(0o600)
    subprocess.run(['systemctl','reload','dbus'],check=True)
    subprocess.run(['systemctl','daemon-reload'],check=True)
    subprocess.run(['systemctl','restart','ducklink-wifi'],check=True)
    switch(release)
    subprocess.run(['systemctl','start','microduck-observer'],check=True)
    after = ready()
    for _ in range(100):
        health=get('health')
        if health.get('bluetooth',{}).get('state')=='ready':break
        time.sleep(.1)
    else:raise RuntimeError('Rust BLE registration not ready')
    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'
    manifest['identityAfter']=sha(identity)
    manifest['bluetooth']=health['bluetooth']
    subprocess.run(['systemctl','disable','ducklink-ble'],check=True)
    assert subprocess.run(['systemctl','is-active','ducklink-ble'],capture_output=True,text=True).stdout.strip()!='active'
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
    for path,data in backups.items():
        if data is None:path.unlink(missing_ok=True)
        else:path.write_bytes(data)
    for path,(uid,gid,mode) in identity_stats.items():os.chown(path,uid,gid);os.chmod(path,mode)
    subprocess.run(['systemctl','reload','dbus'],check=True)
    subprocess.run(['systemctl','daemon-reload'],check=True)
    subprocess.run(['systemctl','restart','ducklink-wifi'],check=True)
    switch(old)
    subprocess.run(['systemctl','start','microduck-observer'],check=True)
    if legacy_enabled=='enabled':subprocess.run(['systemctl','enable','ducklink-ble'],check=True)
    if legacy_active=='active':subprocess.run(['systemctl','start','ducklink-ble'],check=True)
    raise
finally:
    Path('/home/radxa/ducklink-rust-deployment.json').write_text(json.dumps(manifest))
print(json.dumps({'result':manifest['result'],'release':str(current.resolve()),
                  'physicalCalibrationUnchanged':physical_calibration(manifest['calibrationValuesAfter'])==physical_calibration(manifest['calibrationValuesBefore']),
                  'registersUnchanged':manifest['servoRegistersAfter']==manifest['servoRegistersBefore'],
                  'missingSnapshots':sum(bool(s['missing']) for s in manifest['readonlySnapshots'])}))
