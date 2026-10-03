from pathlib import Path
import subprocess
root=Path(__file__).resolve().parents[1]
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
password=next(x.removeprefix('Password: ') for x in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if x.startswith('Password: '))
subprocess.run(['scp',*opts,str(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer'),f'{host}:/home/radxa/observer-servo-official-io.new'],check=True)
subprocess.run(['scp',*opts,str(root/'deploy/install-servo-official-io.py'),f'{host}:/home/radxa/'],check=True)
r=subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/install-servo-official-io.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(r.stdout,r.stderr,flush=True);r.check_returncode()
subprocess.run(['scp',*opts,f'{host}:/home/radxa/servo-official-io-deployment.json',str(root/'docs/实测记录/附件/HAT/2026-10-03/servo-official-io-deployment.json')],check=True)
