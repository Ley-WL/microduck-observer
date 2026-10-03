from pathlib import Path
import subprocess
password=next(x.removeprefix('Password: ') for x in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if x.startswith('Password: '))
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193']
result=subprocess.run(['ssh',*opts,'radxa@192.168.31.186',"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/test-small-oscillation-three-board.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr)
dest=Path(__file__).resolve().parents[1]/'docs/实测记录/附件/HAT/2026-10-03'
for n in range(1,4):
    subprocess.run(['scp',*opts,f'radxa@192.168.31.186:/home/radxa/small-oscillation-repeat-{n}-20261003.json',str(dest/f'small-oscillation-repeat-{n}.json')],check=True)
raise SystemExit(result.returncode)
