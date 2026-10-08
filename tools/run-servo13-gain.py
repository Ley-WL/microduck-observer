from pathlib import Path
import subprocess
import sys

root=Path(__file__).resolve().parents[1]
prefix='servo13-gain'
mode=''
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
for source,target in [(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/examples/servo13_gain_check','servo13-gain-check'),
                      (root/'tools/run-servo13-gain-board.py','run-servo13-gain-board.py')]:
    subprocess.run(['scp',*opts,str(source),f'{host}:/home/radxa/{target}'],check=True)
subprocess.run(['ssh',*opts,host,'chmod 755 /home/radxa/servo13-gain-check'],check=True)
password=next(line.removeprefix('Password: ') for line in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/run-servo13-gain-board.py "+mode],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
directory=root/'docs/实测记录/附件/HAT/2026-10-05/servo13-gain';directory.mkdir(parents=True,exist_ok=True)
for name in [prefix+'-acceptance.json',prefix+'-manifest.json']:
    subprocess.run(['scp',*opts,f'{host}:/home/radxa/{name}',str(directory/name)],check=True)
result.check_returncode()
