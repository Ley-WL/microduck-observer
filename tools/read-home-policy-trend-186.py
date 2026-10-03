"""Read-only snapshots and independent shadow predictions; never starts policy."""
import json, math, time, urllib.request
from pathlib import Path
import numpy as np
base='http://192.168.31.186:8877'
def get(route):
    with urllib.request.urlopen(base+route,timeout=8) as r:return json.load(r)
def rot(q):
    x,y,z,w=np.array(q)/np.linalg.norm(q)
    return np.array([[1-2*(y*y+z*z),2*(x*y-z*w),2*(x*z+y*w)], [2*(x*y+z*w),1-2*(x*x+z*z),2*(y*z-x*w)], [2*(x*z-y*w),2*(y*z+x*w),1-2*(x*x+y*y)]])
cal=get('/api/v1/calibration')
records=[]
ids=[20,21,22,23,24,30,31,32,33,10,11,12,13,14]
for _ in range(3):
    snapshot=get('/api/v1/snapshot')
    req=urllib.request.Request(base+'/api/v1/policy/shadow',json.dumps({'revision':cal['revision']}).encode(),{'Content-Type':'application/json'},method='POST')
    with urllib.request.urlopen(req,timeout=12) as r:result=json.load(r)
    imu=cal['imu']; mounting=rot(imu['mountingQuaternion'])
    body=rot(imu.get('targetQuaternion',[0,0,0,1]))@mounting.T@rot(imu['quaternion']).T@rot(snapshot['imu.orientation']['data']['quaternion'])@mounting
    gravity=body.T@np.array([0.,0.,-1.])
    gyro=mounting.T@np.array(snapshot['imu.raw']['data']['gyro'])
    feedback={r['id']:r for r in snapshot['joints']['data']['servos']}
    angles=[(feedback[i]['position']-cal['joints']['references'][str(i)])*360/4096/cal['joints']['directions'].get(str(i),-1) for i in ids]
    records.append({'snapshot':snapshot,'prediction':result,'angles':angles,'gravity':gravity.tolist(),'gyro':gyro.tolist(),'tilt':math.degrees(math.acos(float(np.clip(-gravity[2],-1,1))))})
    time.sleep(.25)
out=Path(__file__).resolve().parents[1]/'docs/实测记录/附件/平台/2026-10-03/home-standing-policy-trend.json'
out.write_text(json.dumps({'calibration':cal,'ids':ids,'records':records},ensure_ascii=False),encoding='utf-8')
for r in records:print('imu',r['tilt'],r['gravity'],r['gyro'],'saturated',r['prediction']['saturatedIds'])
for n,i in enumerate(ids):
    print(i,'current',round(records[-1]['angles'][n],2),'raw', [round(r['prediction']['rawTargetDegrees'][n],2) for r in records], 'sent',[round(r['prediction']['sentTargetDegrees'][n],2) for r in records])
after=get('/api/v1/health')
print('torque',[(r['id'],r['torque']) for r in after['joints']['data']['servos']])
