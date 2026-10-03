import json
import math
from pathlib import Path
import subprocess
import time
import urllib.request
import numpy as np

root = Path(__file__).resolve().parents[1]
base = 'http://192.168.31.186:8877/api/v1/'
def get(route):
    return json.load(urllib.request.urlopen(base + route, timeout=5))
password = next(x.removeprefix('Password: ') for x in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if x.startswith('Password: '))
result = subprocess.run(['ssh', '-o', 'BatchMode=yes', '-o', 'HostKeyAlias=192.168.31.193', 'radxa@192.168.31.186', "sudo -S -p '' cat /home/radxa/microduck-observer/backups/20261003-power-controls/calibration.json"], input=password+'\n', text=True, capture_output=True, check=True)
before = json.loads(result.stdout)
current = get('calibration')
def rot(q):
    x,y,z,w = np.array(q)/np.linalg.norm(q)
    return np.array([[1-2*(y*y+z*z),2*(x*y-z*w),2*(x*z+y*w)], [2*(x*y+z*w),1-2*(x*x+z*z),2*(y*z-x*w)], [2*(x*z-y*w),2*(y*z+x*w),1-2*(x*x+y*y)]])
samples=[]
for _ in range(10):
    snap=get('snapshot')
    i=current['imu']; m=rot(i['mountingQuaternion'])
    body=rot(i.get('targetQuaternion',[0,0,0,1]))@m.T@rot(i['quaternion']).T@rot(snap['imu.orientation']['data']['quaternion'])@m
    gravity=body.T@np.array([0,0,-1])
    samples.append({'tiltDeg':math.degrees(math.acos(np.clip(-gravity[2],-1,1))), 'gravity':gravity.tolist(), 'joints':snap['joints']['data']['servos'], 'control':snap['system']['data']['servoControl']})
    time.sleep(.1)
report={'before':before,'current':current,'jointsUnchanged':before['joints']==current['joints'], 'imuReferenceUnchanged':before['imu']['quaternion']==current['imu']['quaternion'], 'mountingUnchanged':before['imu'].get('mountingQuaternion')==current['imu'].get('mountingQuaternion'),'samples':samples}
(root/'docs/实测记录/附件/平台/2026-10-03/reboot-tilt-analysis.json').write_text(json.dumps(report),encoding='utf-8')
print(json.dumps({k:v for k,v in report.items() if k not in ('before','current','samples')}))
print('tilt',min(s['tiltDeg'] for s in samples),max(s['tiltDeg'] for s in samples))
print(json.dumps(samples[-1],ensure_ascii=True)[:7000])
