"""Read-only 45-second capture; no actuator commands."""
import subprocess,json
from pathlib import Path
folder=Path(__file__).resolve().parent
host='radxa@8.133.210.70'
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2','-o','HostKeyAlias=192.168.31.193']
script='''import pathlib,json,time,urllib.request
out=pathlib.Path('/home/radxa/getup-readonly-20261011');out.mkdir(exist_ok=True)
path=pathlib.Path('/var/lib/microduck-observer/policy-session-latest.json')
last=None;deadline=time.monotonic()+45;rows=[];saved=[]
while time.monotonic()<deadline:
 try:
  x=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=2))
  rows.append(x)
  stamp=path.stat().st_mtime_ns
  if stamp!=last:
   last=stamp;data=path.read_bytes();trace=json.loads(data)
   kinds=set(f.get('kind') for f in trace['frames'])
   if 'xgoduck_getup' in kinds:
    name='getup-'+str(trace['eventTimeMs'])+'.json'
    (out/name).write_bytes(data);saved.append(name)
 except Exception as e: rows.append({'captureError':str(e)})
 time.sleep(.1)
(out/'snapshots.json').write_text(json.dumps(rows))
print(json.dumps({'snapshots':len(rows),'getupFiles':saved,'lastControl':rows[-1].get('system',{}).get('data',{}).get('servoControl',{})}))
'''
r=subprocess.run(['ssh',*opts,'-p','5722',host,'python3 -'],input=script,text=True,encoding='utf-8',capture_output=True,check=True)
x=json.loads(r.stdout);(folder/'watch-summary.json').write_text(json.dumps(x,ensure_ascii=False,indent=2),encoding='utf-8')
for name in ['snapshots.json',*x['getupFiles']]:
 subprocess.run(['scp','-O',*opts,'-P','5722',host+':/home/radxa/getup-readonly-20261011/'+name,str(folder/name)],check=True)
print(json.dumps(x,ensure_ascii=False,indent=2))
