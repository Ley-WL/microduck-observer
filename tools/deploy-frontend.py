"""Publish only Web assets into an isolated release, with no service restart or motion."""
import argparse, hashlib, json, shlex, subprocess, tarfile, time
from datetime import datetime, timezone, timedelta
from pathlib import Path

root=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--host',default='radxa@8.133.210.70')
parser.add_argument('--port',type=int,default=5722)
parser.add_argument('--package-only',action='store_true')
args=parser.parse_args()
name='frontend-'+datetime.now(timezone(timedelta(hours=8))).strftime('%Y%m%d-%H%M%S')
stage=root/'deploy/staging'/name;stage.mkdir(parents=True,exist_ok=True)
evidence=root/'docs/实测记录/附件/平台'/datetime.now(timezone(timedelta(hours=8))).strftime('%Y-%m-%d')/name
evidence.mkdir(parents=True,exist_ok=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
dist=root/'frontend/dist';assert (dist/'index.html').is_file()
manifest={'name':name,'files':{p.relative_to(dist).as_posix():sha(p) for p in dist.rglob('*') if p.is_file()}}
(stage/'manifest.json').write_text(json.dumps(manifest))
installer=r'''
import hashlib,json,os,shutil,subprocess,tarfile,time,urllib.request
from pathlib import Path
stage=Path('/home/radxa/frontend-stage')
meta=json.loads((stage/'manifest.json').read_text())
name=meta['name'];assert name.startswith('frontend-') and name.replace('-','').isalnum()
root=Path('/home/radxa/microduck-observer');old=(root/'current').resolve();new=root/'releases'/name
assert not new.exists()
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def get():return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
def idle(s):return s['system']['data']['servoControl']['state'] in ('idle','disabled','failed','holding') and all(r['torque']==0 for r in s['joints']['data']['servos'])
before=get();assert idle(before),'Active/enabled robot; leave platform unchanged'
pid=subprocess.check_output(['systemctl','show','microduck-observer','-p','MainPID','--value']).strip()
unchanged={str(p):sha(p) for directory in [old/'bin',old/'models',old/'lib',Path('/var/lib/ducklink')] for p in directory.rglob('*') if p.is_file()}
gains=Path('/var/lib/microduck-observer/model-gains.json')
if gains.exists():unchanged[str(gains)]=sha(gains)
shutil.copytree(old,new,symlinks=True)
target=new/'frontend/dist'
assert target.is_dir() and not target.is_symlink()
previous_assets=target/'assets'
# Keep old hashed files for tabs open during the atomic current-link change.
with tarfile.open(stage/'assets.tar.gz') as archive:
    for m in archive.getmembers():
        path=(target/m.name).resolve();assert path.is_relative_to(target.resolve()) and (m.isfile() or m.isdir())
    archive.extractall(target)
for filename,digest in meta['files'].items():assert sha(target/filename)==digest
for file,digest in unchanged.items():
    p=Path(file);assert sha(p)==digest
    if p.is_relative_to(old):assert sha(new/p.relative_to(old))==digest
assert idle(get()),'Robot changed state; do not switch'
temporary=root/'current.frontend-staging';assert not temporary.exists() and not temporary.is_symlink()
temporary.symlink_to(new)
try:
    os.replace(temporary,root/'current')
    for filename,digest in meta['files'].items():
        raw=urllib.request.urlopen('http://127.0.0.1:8877/'+filename,timeout=8).read();assert hashlib.sha256(raw).hexdigest()==digest
    after=get();assert after['calibration']==before['calibration'] and idle(after)
    assert subprocess.check_output(['systemctl','show','microduck-observer','-p','MainPID','--value']).strip()==pid
    assert all(sha(Path(p))==digest for p,digest in unchanged.items())
    assert all(subprocess.run(['systemctl','is-enabled',svc],capture_output=True,text=True).stdout.strip()=='masked' for svc in ['tofd','rsyslog'])
    report={'installed':True,'release':str(new),'previousRelease':str(old),'assets':meta['files'],'mainPidUnchanged':int(pid),'calibrationUnchanged':True,'modelsAndGainsUnchanged':True,'hardwareMotion':False,'serviceRestart':False}
    (stage/'deployment.json').write_text(json.dumps(report,indent=2));print(json.dumps(report))
except BaseException:
    temporary.symlink_to(old);os.replace(temporary,root/'current');raise
'''
(stage/'install.py').write_text(installer,encoding='utf-8');compile(installer,'install.py','exec')
with tarfile.open(stage/'assets.tar.gz','w:gz') as archive:
    for p in dist.iterdir():archive.add(p,arcname=p.name)
with tarfile.open(stage/'payload.tar.gz','w:gz') as archive:
    for file in ['assets.tar.gz','manifest.json','install.py']:archive.add(stage/file,arcname=file)
if args.package_only:print(stage/'payload.tar.gz');raise SystemExit(0)
options=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2','-o','HostKeyAlias=192.168.31.193']
ssh=['ssh',*options,'-p',str(args.port),args.host];remote='/home/radxa/frontend-stage'
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
for attempt in range(3):
    transfer=subprocess.run(['scp','-O',*options,'-P',str(args.port),str(stage/'payload.tar.gz'),args.host+':'+remote+'/payload.tar.gz'])
    if transfer.returncode==0:break
    if attempt==2:transfer.check_returncode()
    time.sleep(2)
password=next(s.removeprefix('Password: ') for s in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if s.startswith('Password: '))
code="import tarfile,runpy; p='/home/radxa/frontend-stage'; a=tarfile.open(p+'/payload.tar.gz'); assert all(m.isfile() and '/' not in m.name for m in a.getmembers()); a.extractall(p); runpy.run_path(p+'/install.py',run_name='__main__')"
result=subprocess.run([*ssh,"sudo -S -p '' python3 -c "+shlex.quote(code)],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
(evidence/'install.log').write_text(result.stdout+result.stderr,encoding='utf-8');print(result.stdout,result.stderr);result.check_returncode()
subprocess.run(['scp','-O',*options,'-P',str(args.port),args.host+':'+remote+'/deployment.json',str(evidence/'deployment.json')],check=True)
