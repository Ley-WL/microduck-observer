"""Isolated Rust helper deployment; only read-only checks, no network/motor/power action."""
from pathlib import Path
import argparse, hashlib, json, subprocess, tarfile, shlex, re
from datetime import datetime, timezone, timedelta
local_time=timezone(timedelta(hours=8))

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--host',default='radxa@8.133.210.70')
parser.add_argument('--port',type=int,default=5722)
parser.add_argument('--release',default='service-rust-'+datetime.now(local_time).strftime('%Y%m%d-%H%M%S'))
parser.add_argument('--package-only',action='store_true',help='Build the payload without contacting the board')
args=parser.parse_args()
if not re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}',args.release):parser.error('Invalid release name')

root=Path(__file__).resolve().parents[1]
stage=root/'deploy/staging'/args.release
stage.mkdir(parents=True,exist_ok=True)
evidence=root/'docs/实测记录/附件/平台'/datetime.now(local_time).strftime('%Y-%m-%d')/args.release
evidence.mkdir(parents=True,exist_ok=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
files={name:root/'backend-rust/target/aarch64-unknown-linux-gnu/release'/name for name in ['ducklink-wifi','microduck-power-helper']}
for path in files.values():
    header=path.read_bytes()[:20]
    assert header[:4]==b'\x7fELF' and int.from_bytes(header[18:20],'little')==183
(stage/'hashes.json').write_text(json.dumps({name:sha(path) for name,path in files.items()}))
files['hashes.json']=stage/'hashes.json'
(stage/'release.json').write_text(json.dumps({'name':args.release}))
files['release.json']=stage/'release.json'
files['install.py']=root/'deploy/install-service-rust.py'
package=stage/'payload.tar.gz'
with tarfile.open(package,'w:gz') as archive:
    for name,path in files.items():archive.add(path,arcname=name)
if args.package_only:
    print(package)
    raise SystemExit(0)
options=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2','-o','HostKeyAlias=192.168.31.193']
host=args.host;ssh=['ssh',*options,'-p',str(args.port),host]
remote='/home/radxa/service-rust-stage'
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
subprocess.run(['scp','-O',*options,'-P',str(args.port),str(package),host+':'+remote+'/payload.tar.gz'],check=True)
password=next(s.removeprefix('Password: ') for s in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if s.startswith('Password: '))
code=f"import tarfile,runpy; p={remote!r}; a=tarfile.open(p+'/payload.tar.gz'); assert all(m.isfile() and '/' not in m.name for m in a.getmembers()); a.extractall(p); runpy.run_path(p+'/install.py',run_name='__main__')"
result=subprocess.run([*ssh,"sudo -S -p '' /usr/bin/python3 -c "+shlex.quote(code)],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
(evidence/'install.log').write_text(result.stdout+result.stderr,encoding='utf-8')
print(result.stdout,result.stderr);result.check_returncode()
subprocess.run(['scp','-O',*options,'-P',str(args.port),host+':'+remote+'/deployment.json',str(evidence/'deployment.json')],check=True)
(evidence/'source-hashes.json').write_text(json.dumps({str(p.relative_to(root)):sha(p) for p in list(files.values())+[Path(__file__),root/'backend-rust/Cargo.lock',root/'backend-rust/src/bin/ducklink-wifi.rs',root/'backend-rust/src/bin/microduck-power-helper.rs']},indent=2),encoding='utf-8')
