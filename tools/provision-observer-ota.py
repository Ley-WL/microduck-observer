"""Off-board bootstrap: install Rust OTA helper, public key and socket units.

No robot motion, firmware, model or identity edits. Reprovision only unloaded.
"""
import hashlib,json,subprocess,shlex,argparse
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--host',default='radxa@8.133.210.70');parser.add_argument('--port',type=int,default=5722);parser.add_argument('--host-key-alias');parser.add_argument('--password-file',type=Path,default=Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt'));args=parser.parse_args()
root=Path(__file__).resolve().parents[1]
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2']
alias=args.host_key_alias or ('192.168.31.193' if args.host=='radxa@8.133.210.70' else None)
if alias:opts+=['-o','HostKeyAlias='+alias]
host=args.host;ssh=['ssh',*opts,'-p',str(args.port),host]
stage=root/'deploy/staging/observer-ota-bootstrap';stage.mkdir(parents=True,exist_ok=True)
helper=root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer-ota-helper'
sha=hashlib.sha256(helper.read_bytes()).hexdigest()
script=r'''
import json,hashlib,urllib.request,subprocess,shutil,os,sys
from pathlib import Path
stage=Path(sys.argv[1]);expected=sys.argv[2]
s=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5));rows=s['joints']['data']['servos']
assert s['system']['data']['servoControl']['state'] not in ['policy','moving','preflight','stopping','disabling'] and s['joints']['ageMs']<1000 and len(rows)==15 and all(r['online'] and not r['torque'] and r['ageMs']<1000 for r in rows),'Must stop and unload all servos'
binary=stage/'microduck-observer-ota-helper';b=binary.read_bytes();assert hashlib.sha256(b).hexdigest()==expected and b[:6]==b'\x7fELF\x02\x01' and int.from_bytes(b[18:20],'little')==183
dest=Path('/usr/local/lib/microduck-observer-ota-helper');temp=dest.with_suffix('.install');shutil.copyfile(binary,temp);os.chmod(temp,0o755);os.replace(temp,dest)
keys=Path('/usr/local/share/microduck-observer');keys.mkdir(parents=True,exist_ok=True);key=keys/'ota-public.pem'
if key.exists():assert key.read_bytes()==(stage/'ota-public.pem').read_bytes(),'Signing key changed unexpectedly'
shutil.copyfile(stage/'ota-public.pem',key);os.chmod(key,0o644)
work=Path('/var/lib/microduck-observer/ota');work.mkdir(parents=True,exist_ok=True);os.chmod(work,0o755)
for name in ['microduck-observer-ota.socket','microduck-observer-ota@.service']:
 shutil.copyfile(stage/name,Path('/etc/systemd/system')/name)
subprocess.run(['systemctl','daemon-reload'],check=True);subprocess.run(['systemctl','enable','--now','microduck-observer-ota.socket'],check=True)
assert Path('/run/microduck-observer-ota.sock').exists()
print(json.dumps(dict(installed=True,helperSha256=expected,hardwareMotion=False,serviceRestarted=False)))
'''
(stage/'install.py').write_text(script,encoding='utf-8')
remote='/home/radxa/observer-ota-bootstrap'
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
subprocess.run(['scp','-O',*opts,'-P',str(args.port),str(helper),str(root/'deploy/ota-public.pem'),str(root/'deploy/microduck-observer-ota.socket'),str(root/'deploy/microduck-observer-ota@.service'),str(stage/'install.py'),host+':'+remote+'/'],check=True)
password=next(s.removeprefix('Password: ') for s in args.password_file.read_text().splitlines() if s.startswith('Password: '))
result=subprocess.run([*ssh,"sudo -S -p '' python3 "+shlex.quote(remote+'/install.py')+' '+shlex.quote(remote)+' '+sha],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout);print(result.stderr);result.check_returncode()
(stage/'result.json').write_text(result.stdout,encoding='utf-8')
