"""Deploy unified Rust BLE/control backend; preserve identity, frontend and physical state."""
from pathlib import Path
import subprocess

root=Path(__file__).resolve().parents[1]
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
for source,target in [(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer','observer-ducklink-rust.new'),
                      (root/'deploy/install-ducklink-rust.py','install-ducklink-rust.py'),
                      (root.parent/'mobile/gateway/wifi_service.py','ducklink-wifi-unified.py')]:
    subprocess.run(['scp',*opts,str(source),f'{host}:/home/radxa/{target}'],check=True)
password=next(line.removeprefix('Password: ') for line in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/install-ducklink-rust.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
directory=root/'docs/实测记录/附件/平台/2026-10-08/ducklink-rust';directory.mkdir(parents=True,exist_ok=True)
subprocess.run(['scp',*opts,f'{host}:/home/radxa/ducklink-rust-deployment.json',str(directory/'deployment.json')],check=True)
result.check_returncode()
