"""Read-only ONNX bench and sensor shadow run. No motor transport or write API."""
import argparse
import json
import math
from pathlib import Path
import time
import urllib.request

import numpy as np
import onnxruntime as ort


class InvalidObservation(ValueError):
    pass


def unit_quat(value):
    q = np.asarray(value, dtype=float)
    if q.shape != (4,) or not np.isfinite(q).all() or abs(np.linalg.norm(q)-1) > .05:
        raise InvalidObservation('invalid quaternion')
    return q / np.linalg.norm(q)


def multiply(a, b):
    av, bv = a[:3], b[:3]
    return np.r_[a[3]*bv+b[3]*av+np.cross(av,bv), a[3]*b[3]-av@bv]


def rotation(q):
    x,y,z,w = q
    return np.array([[1-2*(y*y+z*z),2*(x*y-z*w),2*(x*z+y*w)],
                     [2*(x*y+z*w),1-2*(x*x+z*z),2*(y*z-x*w)],
                     [2*(x*z-y*w),2*(y*z+x*w),1-2*(x*x+y*y)]])


def joint_angle(position, reference, direction, limits):
    if not all(math.isfinite(v) for v in (position,reference,direction)) or direction not in (-1,1):
        raise InvalidObservation('invalid encoder calibration')
    raw=(position-reference)*2*math.pi/4096*direction
    lo,hi=limits
    # Select a unique periodic equivalent inside the physical joint range.
    k0=math.ceil((lo-raw)/(2*math.pi)); k1=math.floor((hi-raw)/(2*math.pi))
    if k0 != k1:
        raise InvalidObservation(f'encoder angle outside/ambiguous in limits: {raw:.3f}, {limits}')
    return raw+k0*2*math.pi, k0


class ObservationBuilder:
    def __init__(self, metadata, geometry):
        names=metadata['joint_names'].split(',')
        joints={b['joint']['name']:b['joint'] for b in geometry['bodies'] if b.get('joint')}
        self.joints=[joints[n] for n in names]
        self.defaults=np.array([float(v) for v in metadata['default_joint_pos'].split(',')])
        if len(names)!=14 or self.defaults.shape!=(14,):
            raise ValueError('requires 14 joints')
        if metadata.get('observation_names')!='base_ang_vel,projected_gravity,joint_pos,joint_vel,actions,command,head_command,body_command':
            raise ValueError('unsupported observation contract')
        self.previous=None
        self.context=None
        self.C=np.array([[0,1,0],[-1,0,0],[0,0,1]],dtype=float)

    def build(self, snapshot, last_action):
        cal=snapshot['calibration']; imu=snapshot['imu.orientation']; raw=snapshot['imu.raw']; js=snapshot['joints']
        yaw=cal.get('mounting',{}).get('yaw',-90)
        if type(yaw) not in (int,float) or yaw not in (-90,0,90):
            raise InvalidObservation('invalid saved mounting direction')
        a=math.radians(yaw)
        self.C=np.array([[math.cos(a),-math.sin(a),0],[math.sin(a),math.cos(a),0],[0,0,1]])
        context=(cal['deviceId'],cal['revision'],imu['bootId'])
        if context!=self.context:
            self.previous=None; self.context=context
        for sample in (imu,raw,js):
            if not sample['valid'] or sample['source']!='hardware' or not 0 <= sample['ageMs'] < 150 or sample['bootId']!=imu['bootId']:
                raise InvalidObservation('stale/invalid/mixed-session sensor data')
        if cal['imu']['bootId']!=imu['bootId']:
            raise InvalidObservation('IMU reference belongs to another boot')
        q0=unit_quat(cal['imu']['quaternion']); q=unit_quat(imu['data']['quaternion'])
        qrel=multiply(q0*np.array([-1,-1,-1,1]),q)
        gravity=self.C @ rotation(qrel).T @ self.C.T @ np.array([0.,0.,-1.])
        gyro=np.asarray(raw['data']['gyro'],dtype=float)
        if gyro.shape!=(3,) or not np.isfinite(gyro).all():
            raise InvalidObservation('invalid gyro')
        rows={v['id']:v for v in js['data']['servos']}
        angles=[]; stamps=[]; branches=[]; counts=[]
        for joint in self.joints:
            sid=joint['id']; row=rows[sid]; key=str(sid)
            if not row['online'] or row['fault'] or not 0 <= row['ageMs']+js['ageMs'] < 150:
                raise InvalidObservation(f'servo {sid} invalid/stale/fault')
            try:
                angle,branch=joint_angle(row['position'],cal['joints']['references'][key],cal['joints']['directions'].get(key,-1),joint['range'])
            except InvalidObservation as exc:
                raise InvalidObservation(f'servo {sid} ({joint["name"]}): {exc}') from exc
            angles.append(angle); branches.append(branch); counts.append(row['position'])
            stamps.append((js['sampleMonoMs']-row['ageMs'])/1000)
        angles=np.array(angles); stamps=np.array(stamps)
        if self.previous is None:
            self.previous=(angles,stamps,np.zeros(14)); raise InvalidObservation('warming velocity history')
        old,old_t,old_v=self.previous; dt=stamps-old_t
        if np.any(dt < -1e-6) or np.any(dt > .25):
            self.previous=None; raise InvalidObservation('joint time discontinuity')
        changed=dt>1e-5
        velocity=old_v.copy(); velocity[changed]=(angles[changed]-old[changed])/dt[changed]
        if np.any(np.abs(velocity)>30):
            self.previous=None; raise InvalidObservation('joint velocity jump')
        self.previous=(angles,stamps,velocity)
        obs=np.concatenate([self.C@gyro,gravity,angles-self.defaults,velocity,last_action,np.zeros(13)]).astype(np.float32)
        if obs.shape!=(61,) or not np.isfinite(obs).all():
            raise InvalidObservation('invalid observation')
        return obs,dict(raw_encoder=counts,periodic_branches=branches,joint_rad=angles.tolist(),velocity_rad_s=velocity.tolist(),gravity=gravity.tolist(),calibration_revision=cal['revision'])


def session(path):
    options=ort.SessionOptions(); options.intra_op_num_threads=1; options.inter_op_num_threads=1
    s=ort.InferenceSession(str(path),sess_options=options,providers=['CPUExecutionProvider'])
    if s.get_inputs()[0].shape!=[1,61] or s.get_outputs()[0].shape!=[1,14]:
        raise ValueError('unexpected model dimensions')
    return s


def infer(s, obs):
    start=time.perf_counter()
    out=s.run(None,{s.get_inputs()[0].name:obs.reshape(1,61)})[0]
    ms=(time.perf_counter()-start)*1000
    if out.shape!=(1,14) or not np.isfinite(out).all():
        raise ValueError('invalid model output')
    return out[0],ms


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--models',type=Path,required=True); p.add_argument('--geometry',type=Path,required=True)
    p.add_argument('--endpoint',default='http://127.0.0.1:8877'); p.add_argument('--seconds',type=float,default=30)
    p.add_argument('--output',type=Path,required=True)
    args=p.parse_args(); args.output.mkdir(parents=True,exist_ok=True)
    summary={'mode':'read-only shadow; NO motor execution','bench':{},'live_policy':'stand','live_success':0,'live_rejected':0,'reasons':{},'limitations':['Open-loop sensor shadow does not validate balance or action feedback.','Uses display-zero pose and board mounting direction; not a hardware control calibration.','Joint velocities are finite differences of asynchronous polling, not measured motor velocities.','Previous action is the previous prediction, never an executed command.']}
    sessions={}
    for path in sorted(args.models.glob('*.onnx')):
        s=session(path); sessions[path.stem]=s
        obs=np.zeros(61,dtype=np.float32); obs[5]=-1
        for _ in range(20): infer(s,obs)
        times=[infer(s,obs)[1] for _ in range(200)]
        summary['bench'][path.stem]={'synthetic_input':True,'p50_ms':float(np.percentile(times,50)),'p95_ms':float(np.percentile(times,95)),'max_ms':max(times)}
    s=sessions['stand']; builder=ObservationBuilder(s.get_modelmeta().custom_metadata_map,json.loads(args.geometry.read_text(encoding='utf-8')))
    previous=np.zeros(14,dtype=np.float32); end=time.monotonic()+args.seconds; durations=[]
    with (args.output/'shadow.jsonl').open('w',encoding='utf-8') as log:
        while time.monotonic()<end:
            tick=time.monotonic(); record={'time':time.time(),'executed':False}
            try:
                with urllib.request.urlopen(args.endpoint.rstrip('/')+'/api/v1/snapshot',timeout=2) as response: snapshot=json.load(response)
                obs,details=builder.build(snapshot,previous); action,ms=infer(s,obs); previous=action
                record.update(status='inferred',obs=obs.tolist(),actions=action.tolist(),inference_ms=ms,**details)
                summary['live_success']+=1; durations.append(ms)
            except (ValueError,KeyError,TypeError,OSError) as exc:
                previous=np.zeros(14,dtype=np.float32)
                reason=str(exc)
                if reason != 'warming velocity history': builder.previous=None
                record.update(status='blocked',reason=reason)
                summary['live_rejected']+=1; summary['reasons'][reason]=summary['reasons'].get(reason,0)+1
            log.write(json.dumps(record,ensure_ascii=False)+'\n'); log.flush()
            time.sleep(max(0,.02-(time.monotonic()-tick)))
    if durations: summary['live_p95_ms']=float(np.percentile(durations,95))
    (args.output/'summary.json').write_text(json.dumps(summary,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    print(json.dumps(summary,indent=2,ensure_ascii=False))


if __name__=='__main__': main()
