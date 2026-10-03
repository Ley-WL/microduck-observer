import json
from pathlib import Path
import subprocess
import time
import urllib.request

root=Path(__file__).resolve().parents[1]
dest=root/'docs/实测记录/附件/平台/2026-10-03'
password=next(x.removeprefix('Password: ') for x in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if x.startswith('Password: '))
def admin(command):
    r=subprocess.run(['ssh','-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193','radxa@192.168.31.186',"sudo -S -p '' "+command],input=password+'\n',text=True,encoding='utf-8',capture_output=True,check=True)
    return r.stdout
data=json.loads(admin('cat /var/lib/microduck-observer/policy-failure-latest.json'))
(dest/'policy-failure-after-roll.json').write_text(json.dumps(data),encoding='utf-8')
snapshots=[]
for _ in range(10):
    snapshots.append(json.load(urllib.request.urlopen('http://192.168.31.186:8877/api/v1/snapshot',timeout=5)))
    time.sleep(.1)
kernel=admin('cat /proc/tty/driver/serial')
(dest/'policy-after-roll-poststate.json').write_text(json.dumps({'snapshots':snapshots,'kernel':kernel}),encoding='utf-8')
print(json.dumps({k:v for k,v in data.items() if k not in ('serialTrace','feedback','previousFeedback','lastInferenceSensors','lastInferenceObservation','jointCalibration','imuCalibration')},ensure_ascii=True))
print('trace entries',len(data.get('serialTrace',[])))
print(json.dumps(data.get('serialTrace',[]),ensure_ascii=True)[-11000:])
print('post complete',sum(all(r['online'] for r in s['joints']['data']['servos']) for s in snapshots))
print('post torque',[(r['id'],r.get('torque'),r.get('voltage')) for r in snapshots[-1]['joints']['data']['servos']])
print(kernel)
