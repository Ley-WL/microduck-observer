"""User supported, feet on table: one symmetric 1 degree ankle probe and restore."""
import json,time,urllib.request,math
from pathlib import Path
import numpy as np
base='http://192.168.31.186:8877/api/v1/'
out={'commands':[],'stages':{},'started':time.time()}
dest=Path(__file__).resolve().parents[1]/'docs/实测记录/附件/IMU/2026-10-03/standing-ankle-progressive.json'
def get(route): return json.load(urllib.request.urlopen(base+route,timeout=5))
def post(route,body):
    result=json.load(urllib.request.urlopen(urllib.request.Request(base+route,json.dumps(body).encode(),{'Content-Type':'application/json'}),timeout=8))
    out['commands'].append({'route':route,'body':body,'result':result,'time':time.time()});return result
def safe(s):
    assert s['system']['data']['servoControl']['state'] not in ('policy','moving','preflight','disabling')
    for r in s['joints']['data']['servos']:
        assert r['online'] and r['fault']==0 and 4<=r['voltage']<=8.4
        assert r['torque']==(0 if r['id']==34 else 1)
def rot(q):
    x,y,z,w=np.array(q)/np.linalg.norm(q)
    return np.array([[1-2*(y*y+z*z),2*(x*y-z*w),2*(x*z+y*w)],[2*(x*y+z*w),1-2*(x*x+z*z),2*(y*z-x*w)],[2*(x*z-y*w),2*(y*z+x*w),1-2*(x*x+y*y)]])
def sample(name):
    rows=[];out['stages'][name]=rows
    for _ in range(15):
        s=get('snapshot');rows.append(s);safe(s);time.sleep(.1)
    return rows
try:
    cal=get('calibration');out['calibration']=cal
    initial=sample('baseline')[-1];servo={r['id']:r for r in initial['joints']['data']['servos']}
    goals={id:(cal['joints']['references'][str(id)]-servo[id]['target'])*360/4096 for id in (14,24)}
    m=rot(cal['imu']['mountingQuaternion'])
    def pitch(rows):
        values=[]
        for s in rows:
            a=np.array(s['imu.raw']['data']['accel']);g=m.T@(-a/np.linalg.norm(a));values.append(math.degrees(math.atan2(g[0],-g[2])))
        return float(np.median(values))
    previous=pitch(out['stages']['baseline']);out['steps']=[]
    for step in range(1,9):
        for id,delta in [(14,-step),(24,step)]:
            safe(get('snapshot'))
            post('servos/angle',{'revision':cal['revision'],'id':id,'angleDeg':goals[id]+delta})
        time.sleep(.7);rows=sample('step-'+str(step));current=pitch(rows)
        out['steps'].append({'step':step,'pitchMedian':current})
        dest.write_text(json.dumps(out),encoding='utf-8')
        print(json.dumps(out['steps'][-1]),flush=True)
        if abs(current)<=2:
            out['result']='near-upright-retained';break
        if current>previous+2:
            for id,delta in [(14,-(step-1)),(24,step-1)]:
                post('servos/angle',{'revision':cal['revision'],'id':id,'angleDeg':goals[id]+delta})
            out['result']='worsened-restored-previous-step';break
        previous=current
    else:out['result']='eight-small-steps-retained'
except Exception as e:
    out['error']=str(e)
    try:out['unload']=post('servos/disable',{})
    except Exception as off:out['unloadError']=str(off)
finally:
    try:out['final']=get('snapshot')
    except Exception:pass
    dest.write_text(json.dumps(out),encoding='utf-8')
report={'result':out.get('result'),'error':out.get('error'),'commands':len(out['commands'])}
if 'calibration' in out:
    m=rot(out['calibration']['imu']['mountingQuaternion'])
    for name,rows in out['stages'].items():
        pitches=[]
        for s in rows:
            a=np.array(s['imu.raw']['data']['accel']);g=m.T@(-a/np.linalg.norm(a));pitches.append(math.degrees(math.atan2(g[0],-g[2])))
        report[name]={'pitchMean':float(np.mean(pitches)),'pitchRange':[min(pitches),max(pitches)]} if pitches else {}
print(json.dumps(report,ensure_ascii=False))
