"""User confirmed the supported torso is upright: update only shared IMU pose reference."""
import json,math,time,urllib.request,datetime
from pathlib import Path
base='http://192.168.31.186:8877/api/v1/'
root=Path(__file__).resolve().parents[1]
directory=root/'docs/实测记录/附件/HAT/2026-10-04/static-stance-supported'
def get():return json.load(urllib.request.urlopen(base+'snapshot',timeout=5))
before=get();report={'before':before,'referenceSamples':[]}
directory.mkdir(parents=True,exist_ok=True)
(directory/'calibration-before.json').write_text(json.dumps(before['calibration']),encoding='utf-8')
boot=before['joints']['bootId'];revision=before['calibration']['revision'];reference=None;values=[]
for _ in range(20):
    snapshot=get()
    assert snapshot['joints']['bootId']==boot and snapshot['calibration']['revision']==revision
    assert snapshot['system']['data']['servoControl']['state'] not in ('policy','moving','preflight','disabling')
    row=snapshot['imu.orientation'];assert row['valid'] and row['ageMs']<150
    quaternion=row['data']['quaternion']
    if reference is None:reference=quaternion
    if sum(a*b for a,b in zip(reference,quaternion))<0:quaternion=[-v for v in quaternion]
    values.append(quaternion);report['referenceSamples'].append(row);time.sleep(.025)
average=[sum(q[n] for q in values)/len(values) for n in range(4)]
norm=math.sqrt(sum(v*v for v in average));average=[v/norm for v in average]
imu=dict(before['calibration']['imu']);imu.pop('validForBoot',None)
imu.update(quaternion=average,bootId=boot,time=datetime.datetime.now(datetime.timezone.utc).isoformat(),targetQuaternion=[0,0,0,1])
body={'revision':revision,'patch':{'imu':imu}}
report['request']=body
(directory/'imu-reference-update.json').write_text(json.dumps(report),encoding='utf-8')
request=urllib.request.Request(base+'calibration',data=json.dumps(body).encode(),headers={'Content-Type':'application/json'},method='POST')
report['result']=json.load(urllib.request.urlopen(request,timeout=6))
time.sleep(.1);report['after']=get()
assert report['result']['joints']==before['calibration']['joints']
assert report['result']['mounting']==before['calibration']['mounting']
assert report['result']['imu']['mountingQuaternion']==before['calibration']['imu']['mountingQuaternion']
keys=['torque','goalPositionRaw','goalCurrentRaw','accelerationRaw','speedLimitRaw','torqueLimitRaw','kpRaw','kiRaw','kdRaw']
def registers(snapshot):return {r['id']:{key:r[key] for key in keys} for r in snapshot['joints']['data']['servos'] if r['online']}
report['registersUnchanged']=registers(before)==registers(report['after'])
(directory/'imu-reference-update.json').write_text(json.dumps(report),encoding='utf-8')
print(json.dumps({'revisionBefore':revision,'revisionAfter':report['result']['revision'],'registersUnchanged':report['registersUnchanged'],'quaternion':average}))
