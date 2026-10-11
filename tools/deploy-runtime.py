"""Overlay Rust binary and Web assets on the current board release; retain models/settings."""
import argparse, hashlib, json, shlex, subprocess, tarfile
from datetime import datetime, timezone, timedelta
from pathlib import Path

root=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--host',default='radxa@192.168.31.186')
parser.add_argument('--port',type=int,default=22)
parser.add_argument('--package-only',action='store_true')
parser.add_argument('--binary-only',action='store_true',help='Retain the current board frontend without overlaying local assets')
args=parser.parse_args()
name='runtime-'+datetime.now(timezone(timedelta(hours=8))).strftime('%Y%m%d-%H%M%S')
stage=root/'deploy/staging'/name;stage.mkdir(parents=True)
evidence=root/'docs/实测记录/附件/平台'/datetime.now(timezone(timedelta(hours=8))).strftime('%Y-%m-%d')/name
evidence.mkdir(parents=True)
binary=root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer'
assert binary.read_bytes()[:4]==b'\x7fELF' and int.from_bytes(binary.read_bytes()[18:20],'little')==183
with tarfile.open(stage/'payload.tar.gz','w:gz') as archive:
    archive.add(binary,arcname='bin/microduck-observer')
    if not args.binary_only:
        archive.add(root/'frontend/dist',arcname='frontend/dist')
    for tool in ['verify_rust_hardware.py','measure_runtime.py','ws_reader.py']:
        archive.add(root/'deploy'/tool,arcname=tool)
installer=r'''
import hashlib,json,os,shutil,subprocess,sys,tarfile,time,urllib.request
from pathlib import Path
stage=Path(sys.argv[1]);name=sys.argv[2];root=Path('/home/radxa/microduck-observer')
assert name.startswith('runtime-') and name.replace('-','').isalnum()
def get(path):return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/'+path,timeout=5))
def idle(s):return s['system']['data']['servoControl']['state'] not in ['preflight','moving','policy','stopping','disabling'] and len(s['joints']['data']['servos'])==15 and all(r['online'] and not r['torque'] for r in s['joints']['data']['servos'])
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
before=get('snapshot');assert idle(before),'Active robot; leave production unchanged'
old=(root/'current').resolve();new=root/'releases'/name;assert not new.exists()
preserved={str(p):sha(p) for base in [old/'models',Path('/var/lib/ducklink')] for p in base.rglob('*') if p.is_file()}
for p in Path('/var/lib/microduck-observer').glob('*.json'):preserved[str(p)]=sha(p)
shutil.copytree(old,new,symlinks=True)
with tarfile.open(stage/'payload.tar.gz') as archive:
    assert all((new/m.name).resolve().is_relative_to(new.resolve()) and (m.isfile() or m.isdir()) for m in archive.getmembers())
    archive.extractall(new)
os.chmod(new/'bin/microduck-observer',0o755)
assert all(sha(Path(p))==v for p,v in preserved.items())
shutil.copy2(new/'measure_runtime.py',stage/'measure_runtime.py')
shutil.copy2(new/'ws_reader.py',stage/'ws_reader.py')
subprocess.run([sys.executable,str(new/'verify_rust_hardware.py'),'--release',str(new),'--workspace',str(stage)],check=True)
assert idle(get('snapshot'))
for tool in ['verify_rust_hardware.py','measure_runtime.py','ws_reader.py']:(new/tool).unlink()
drop=Path('/etc/systemd/system/microduck-observer.service.d/audio.conf');prior=drop.read_bytes() if drop.exists() else None
audio=b'[Service]\nSupplementaryGroups=audio\nDeviceAllow=/dev/snd/controlC1 rw\nDeviceAllow=/dev/snd/pcmC1D0p rw\n'
def point(p):
    temp=root/'current.runtime-staging';assert not temp.exists() and not temp.is_symlink();temp.symlink_to(p);os.replace(temp,root/'current')
try:
    subprocess.run(['systemctl','stop','microduck-observer'],check=True)
    drop.write_bytes(audio);point(new)
    subprocess.run(['systemctl','daemon-reload'],check=True);subprocess.run(['systemctl','start','microduck-observer'],check=True)
    for _ in range(80):
        try:
            after=get('snapshot')
            if idle(after):break
        except Exception:pass
        time.sleep(.15)
    else:raise RuntimeError('New runtime did not become ready')
    assert after['calibration']['revision']==before['calibration']['revision']
    for key in ['joints','mounting']:assert after['calibration'][key]==before['calibration'][key]
    for key in ['quaternion','mountingQuaternion','targetQuaternion']:assert after['calibration']['imu'].get(key)==before['calibration']['imu'].get(key)
    assert all(sha(Path(p))==v for p,v in preserved.items())
    for p,v in preserved.items():
        p=Path(p)
        if p.is_relative_to(old):assert sha(new/p.relative_to(old))==v
    assets={p.relative_to(new/'frontend/dist').as_posix():sha(p) for p in (new/'frontend/dist').rglob('*') if p.is_file()}
    for p,v in assets.items():assert hashlib.sha256(urllib.request.urlopen('http://127.0.0.1:8877/'+p,timeout=8).read()).hexdigest()==v
    snapshots=[get('snapshot') for _ in range(20)];assert all(idle(s) for s in snapshots)
    assert all(subprocess.run(['systemctl','is-enabled',s],capture_output=True,text=True).stdout.strip()=='masked' for s in ['tofd','rsyslog'])
    result={'installed':True,'release':str(new),'previousRelease':str(old),'calibrationModelsGainsIdentityUnchanged':True,'assetsHttpVerified':True,'readonlySnapshots':20,'hardwareMotion':False,'audioPermissionsConfigured':True,'binarySha256':sha(new/'bin/microduck-observer')}
    (stage/'deployment.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
except BaseException:
    subprocess.run(['systemctl','stop','microduck-observer'])
    if prior is None:drop.unlink(missing_ok=True)
    else:drop.write_bytes(prior)
    point(old);subprocess.run(['systemctl','daemon-reload']);subprocess.run(['systemctl','start','microduck-observer']);raise
'''
(stage/'install.py').write_text(installer,encoding='utf-8');compile(installer,'install.py','exec')
if args.package_only:print(stage);raise SystemExit(0)
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2','-o','HostKeyAlias=192.168.31.193']
ssh=['ssh',*opts,'-p',str(args.port),args.host];remote='/home/radxa/runtime-stage-'+name
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
subprocess.run(['scp','-O',*opts,'-P',str(args.port),str(stage/'payload.tar.gz'),str(stage/'install.py'),args.host+':'+remote+'/'],check=True)
password=next(s.removeprefix('Password: ') for s in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if s.startswith('Password: '))
result=subprocess.run([*ssh,"sudo -S -p '' python3 "+shlex.quote(remote+'/install.py')+' '+shlex.quote(remote)+' '+name],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout);print(result.stderr);result.check_returncode()
report=json.loads(result.stdout.splitlines()[-1]);(evidence/'deployment.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
