import json, math, time, sys
from pathlib import Path
from urllib.request import urlopen
import numpy as np

root=Path(__file__).resolve().parent
prefix=(sys.argv[1]+'-') if len(sys.argv)>1 else ''
def rotation(q):
    x,y,z,w=np.array(q,dtype=float)/np.linalg.norm(q)
    return np.array([[1-2*(y*y+z*z),2*(x*y-z*w),2*(x*z+y*w)],
                     [2*(x*y+z*w),1-2*(x*x+z*z),2*(y*z-x*w)],
                     [2*(x*z-y*w),2*(y*z+x*w),1-2*(x*x+y*y)]])
def angle(a,b):
    return math.degrees(math.acos(float(np.clip(np.dot(a,b)/np.linalg.norm(a)/np.linalg.norm(b),-1,1))))
rows=[]
for _ in range(60 if prefix else 30):
    snapshot=json.load(urlopen('http://192.168.31.186:8877/api/v1/snapshot',timeout=5))
    rows.append(snapshot)
    time.sleep(.1)
(root/(prefix+'readonly-snapshots.json')).write_text(json.dumps(rows),encoding='utf-8')
results=[]
for s in rows:
    imu=s['calibration']['imu']
    r=rotation(s['imu.orientation']['data']['quaternion'])
    m=rotation(imu['mountingQuaternion']);t=rotation(imu.get('targetQuaternion',[0,0,0,1]))
    ref=rotation(imu['quaternion'])
    accel=np.array(s['imu.raw']['data']['accel']); gyro=np.array(s['imu.raw']['data']['gyro'])
    up=np.array([0.,0.,1.])
    body=t@m.T@ref.T@r@m
    results.append(dict(revision=s['calibration']['revision'],boot=s['system']['bootId'],
        controlState=s['system']['data']['servoControl']['state'],
        calibratedTiltDeg=angle(body@up,up),
        mountingAccelTiltDeg=angle(m.T@accel,up),
        quaternionAccelWorldTiltDeg=angle(r@accel,up),
        savedVirtualUpTiltDeg=angle(ref@m@t.T@up,up),
        accelNorm=float(np.linalg.norm(accel)),gyroNorm=float(np.linalg.norm(gyro))))
summary=dict(samples=len(rows),referenceChanged=any(s['calibration']['imu']!=rows[0]['calibration']['imu'] for s in rows),
             bootChanged=len({r['boot'] for r in results})>1,motionIssued=False,
             limitation='Current passive posture only; no upright-before/lift/return paired trial')
summary['quasiStaticSamples']=sum(r['gyroNorm']<.03 and abs(r['accelNorm']-9.80665)<.3 for r in results)
stable=[r for r in results if r['gyroNorm']<.03 and abs(r['accelNorm']-9.80665)<.3]
if stable:
    summary['quasiStaticStats']={k:dict(min=min(r[k] for r in stable),median=float(np.median([r[k] for r in stable])),max=max(r[k] for r in stable))
        for k in ['quaternionAccelWorldTiltDeg','calibratedTiltDeg','mountingAccelTiltDeg']}
for k in ['calibratedTiltDeg','mountingAccelTiltDeg','quaternionAccelWorldTiltDeg','savedVirtualUpTiltDeg','accelNorm','gyroNorm']:
    values=[r[k] for r in results];summary[k]=dict(min=min(values),median=float(np.median(values)),max=max(values))
(root/(prefix+'analysis.json')).write_text(json.dumps(dict(summary=summary,rows=results),indent=2),encoding='utf-8')
print(json.dumps(summary))
