"""Isolated Rust helper deployment; only read-only checks, no network/motor/power action."""
from pathlib import Path
import hashlib, json, subprocess, tarfile, shlex

root=Path(__file__).resolve().parents[1]
stage=root/'deploy/staging/service-rust-20261011'
stage.mkdir(parents=True,exist_ok=True)
evidence=root/'docs/实测记录/附件/平台/2026-10-11/service-rust'
evidence.mkdir(parents=True,exist_ok=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
files={name:root/'backend-rust/target/aarch64-unknown-linux-gnu/release'/name for name in ['ducklink-wifi','microduck-power-helper']}
for path in files.values():
    header=path.read_bytes()[:20]
    assert header[:4]==b'\x7fELF' and int.from_bytes(header[18:20],'little')==183
(stage/'hashes.json').write_text(json.dumps({name:sha(path) for name,path in files.items()}))
files['hashes.json']=stage/'hashes.json'
files['install.py']=root/'deploy/install-service-rust.py'
package=stage/'payload.tar.gz'
with tarfile.open(package,'w:gz') as archive:
    for name,path in files.items():archive.add(path,arcname=name)
options=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2','-o','HostKeyAlias=192.168.31.193']
host='radxa@8.133.210.70';ssh=['ssh',*options,'-p','5722',host]
remote='/home/radxa/service-rust-stage'
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
subprocess.run(['scp','-O',*options,'-P','5722',str(package),host+':'+remote+'/payload.tar.gz'],check=True)
password=next(s.removeprefix('Password: ') for s in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if s.startswith('Password: '))
code=f"import tarfile,runpy; p={remote!r}; a=tarfile.open(p+'/payload.tar.gz'); assert all(m.isfile() and '/' not in m.name for m in a.getmembers()); a.extractall(p); runpy.run_path(p+'/install.py',run_name='__main__')"
result=subprocess.run([*ssh,"sudo -S -p '' /usr/bin/python3 -c "+shlex.quote(code)],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
(evidence/'install.log').write_text(result.stdout+result.stderr,encoding='utf-8')
print(result.stdout,result.stderr);result.check_returncode()
subprocess.run(['scp','-O',*options,'-P','5722',host+':'+remote+'/deployment.json',str(evidence/'deployment.json')],check=True)
(evidence/'source-hashes.json').write_text(json.dumps({str(p.relative_to(root)):sha(p) for p in list(files.values())+[Path(__file__),root/'backend-rust/Cargo.lock',root/'backend-rust/src/bin/ducklink-wifi.rs',root/'backend-rust/src/bin/microduck-power-helper.rs']},indent=2),encoding='utf-8')
