import json
from pathlib import Path
import subprocess
import urllib.request
password=next(x.removeprefix('Password: ') for x in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if x.startswith('Password: '))
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193']
r=subprocess.run(['ssh',*opts,'radxa@192.168.31.186',"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/test-small-oscillation-board.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(r.stdout,r.stderr)
dest=Path(__file__).resolve().parents[1]/'docs/实测记录/附件/HAT/2026-10-03'
dest.mkdir(parents=True,exist_ok=True)
subprocess.run(['scp',*opts,'radxa@192.168.31.186:/home/radxa/small-oscillation-20261003.json',str(dest/'small-oscillation.json')],check=True)
health=json.load(urllib.request.urlopen('http://192.168.31.186:8877/api/v1/health',timeout=5))
(dest/'small-oscillation-health.json').write_text(json.dumps(health),encoding='utf-8')
print('After',[(row['id'],row.get('online'),row.get('torque')) for row in health['joints']['data']['servos']])
raise SystemExit(r.returncode)
