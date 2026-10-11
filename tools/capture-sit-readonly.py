"""Read-only remote state sampling; never sends control or motion requests."""
import subprocess
from pathlib import Path

root=Path(__file__).resolve().parents[1]
remote='''import json,time,urllib.request
rows=[]
for _ in range(500):
    at=time.time()
    try:
        s=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=1))
        rows.append({'at':at,'control':s['system']['data']['servoControl'],'servos':s['joints']['data']['servos'],'imu':s.get('imu.orientation')})
    except Exception as e: rows.append({'at':at,'error':str(e)})
    time.sleep(max(0,.1-(time.time()-at)))
logs=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/logs?limit=100',timeout=2))
print(json.dumps({'samples':rows,'logs':logs}))
'''
result=subprocess.run(['ssh','-o','BatchMode=yes','-o','ConnectTimeout=8','-o','HostKeyAlias=192.168.31.193','-p','5722','radxa@8.133.210.70','python3 -'],input=remote,text=True,encoding='utf-8',capture_output=True,check=True)
path=root/'docs/实测记录/附件/平台/2026-10-10/bidirectional-sit-inspect/timeline.json'
path.write_text(result.stdout,encoding='utf-8')
print('Read-only timeline saved.')
