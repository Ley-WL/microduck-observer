"""Repair only two verified missing audio overlays; no motor commands."""
import hashlib,json,shutil,subprocess,urllib.request
from pathlib import Path

stage=Path('/home/radxa/audio-repair-20261013')
report=stage/'installed.json'
assert not report.exists(),'Follow the existing repair rather than re-install/reboot'
expected={
    'rk3568-i2c3-pihat.dtbo':'9a7f9ab7d49d5a77459e4943f8533cdffd5df7b6bca2bc04f331e492a096d27d',
    'rk3568-aic3104-i2c3.dtbo':'89331fb674763ced9e9999e97a4de2fcfac4d8c1ff469ec33e9a4fbbacf9d848',
}
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot():return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
def idle(s):
    servos=s['joints']['data']['servos']
    return s['system']['data']['servoControl']['state'] in ('idle','disabled','failed','holding') and len(servos)==15 and all(v['online'] and not v['torque'] for v in servos) and s['joints']['ageMs']<1000
before=snapshot();assert idle(before),'Device must be idle with all15 motors unloaded'
env=Path('/boot/armbianEnv.txt');boot=env.read_bytes()
assert b'i2c3-pihat' in boot and b'aic3104-i2c3' in boot
assert sha(Path('/boot/dtb/rockchip/rk3566-radxa-zero3.dtb'))=='c7e02c515bb06773878530fc28eafcab962fe6aa5e240ad30a2800c5216707ac'
for name,digest in expected.items():
    assert sha(stage/name)==digest
    assert not (Path('/boot/dtb/rockchip/overlay')/name).exists(),'Only repair missing files'
subprocess.run(['fdtoverlay','-i','/boot/dtb/rockchip/rk3566-radxa-zero3.dtb','-o',str(stage/'merged-test.dtb'),*[str(stage/name) for name in expected]],check=True)
release=Path('/home/radxa/microduck-observer/current').resolve()
preserved={str(p):sha(p) for folder in [release/'models',release/'bin',Path('/var/lib/microduck-observer'),Path('/var/lib/ducklink')] for p in folder.rglob('*') if p.is_file()}
shutil.copy2(env,stage/'armbianEnv.before.txt')
for name,digest in expected.items():
    dest=Path('/boot/dtb/rockchip/overlay')/name
    shutil.copy2(stage/name,dest);dest.chmod(0o644)
    assert sha(dest)==digest
assert env.read_bytes()==boot
assert all(sha(Path(p))==h for p,h in preserved.items())
assert idle(snapshot()),'Do not reboot an active device'
result=dict(installed=True,overlays=expected,bootEnvironmentUnchanged=True,offlineMergePassed=True,
            preservedSha256=preserved,calibration=before['calibration'],hardwareMotion=False,
            previousBootId=before['joints']['bootId'],rebootRequired=True)
report.write_text(json.dumps(result,indent=2))
print(json.dumps(dict(installed=True,offlineMergePassed=True,modelsCalibrationIdentityGainsUnchanged=True,hardwareMotion=False,rebootRequired=True)))
subprocess.run(['systemctl','reboot'],check=True)
