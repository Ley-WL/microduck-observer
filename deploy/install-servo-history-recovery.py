"""Install HD1910 I/O scheduling and feedback fallback update, preserving current assets/config/state."""
import hashlib,json,os,shutil,subprocess,time,urllib.request
from pathlib import Path

root=Path('/home/radxa/microduck-observer')
current=root/'current';old=current.resolve()
release=root/'releases/20261003-servo-history-recovery-v2'
cal=Path('/var/lib/microduck-observer/calibration.json')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def assets(p):
    return {str(f.relative_to(p)):sha(f) for top in ('frontend','models') for f in (p/top).rglob('*') if f.is_file()}
before=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
assert before['system']['data']['servoControl']['state'] not in ('policy','moving','preflight','disabling')
assert all(r['online'] and r['torque'] in (0,1) for r in before['joints']['data']['servos'])
register_keys=('torque','goalPositionRaw','goalCurrentRaw','accelerationRaw','speedLimitRaw','torqueLimitRaw','kpRaw','kiRaw','kdRaw')
registers_before={str(r['id']):{k:r[k] for k in register_keys} for r in before['joints']['data']['servos']}
assert current.is_symlink() and not release.exists()
manifest={'previous':str(old),'release':str(release),'calibrationBefore':sha(cal),'assetsBefore':assets(old)}
shutil.copytree(old,release,symlinks=True)
shutil.copy2('/home/radxa/observer-servo-history-recovery.new',release/'bin/microduck-observer')
(release/'bin/microduck-observer').chmod(0o755)
assert assets(release)==manifest['assetsBefore']
manifest['binarySha256']=sha(release/'bin/microduck-observer')
subprocess.run(['systemctl','stop','microduck-observer'],check=True)
def switch(path):
    temp=root/'current.history-recovery-next'
    assert not temp.exists() and not temp.is_symlink()
    temp.symlink_to(path);os.replace(temp,current)
try:
    switch(release)
    subprocess.run(['systemctl','start','microduck-observer'],check=True)
    for n in range(40):
        try:
            health=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/health',timeout=2))
            snapshot=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=2))
            if 'joints' in snapshot and snapshot['joints']['data'].get('servos'):break
        except Exception:pass
        time.sleep(.25)
    else:raise RuntimeError('新服务启动/采集未就绪')
    assert sha(cal)==manifest['calibrationBefore']
    registers_after={str(r['id']):{k:r[k] for k in register_keys} for r in snapshot['joints']['data']['servos'] if r['online']}
    assert registers_after == registers_before
    manifest.update({'servoRegistersBefore':registers_before,'servoRegistersAfter':registers_after})
    manifest.update({'calibrationAfter':sha(cal),'assetsAfter':assets(release),'health':health,'snapshot':snapshot,'result':'installed'})
except Exception:
    subprocess.run(['systemctl','stop','microduck-observer'],check=True)
    switch(old);subprocess.run(['systemctl','start','microduck-observer'],check=True)
    raise
finally:
    Path('/home/radxa/servo-history-recovery-deployment.json').write_text(json.dumps(manifest))
print(json.dumps({'result':manifest.get('result'),'release':str(current.resolve()),'binarySha256':manifest['binarySha256'],'calibrationUnchanged':manifest.get('calibrationAfter')==manifest['calibrationBefore']}))
