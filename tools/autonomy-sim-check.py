"""CPU policy rollout against the official XML PD model, NOT HD1910 transfer validation."""
import argparse
import json
from pathlib import Path
import numpy as np
import mujoco
import onnxruntime as ort

p=argparse.ArgumentParser()
p.add_argument('--scene',type=Path,required=True)
p.add_argument('--models',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
args=p.parse_args();args.output.mkdir(parents=True,exist_ok=True)
summary={'mode':'CPU MuJoCo official XML PD; no hardware IO',
         'limitations':['Uses XML position actuators, not trained BAM or measured HD1910 dynamics.',
                        'A nominal rollout cannot establish real-robot balance or transfer.'], 'cases':[]}
for policy_name in ['alpha_stand','alpha_sitstand']:
    opts=ort.SessionOptions();opts.intra_op_num_threads=1
    policy=ort.InferenceSession(str(args.models/(policy_name+'.onnx')),sess_options=opts)
    meta=policy.get_modelmeta().custom_metadata_map
    if policy.get_inputs()[0].shape != [1,61] or policy.get_outputs()[0].shape != [1,14]:
        raise ValueError('Unsupported policy shape')
    for pose in ['STAND','SIT']:
        m=mujoco.MjModel.from_xml_path(str(args.scene));m.opt.timestep=.005
        d=mujoco.MjData(m)
        mujoco.mj_resetDataKeyframe(m,d,mujoco.mj_name2id(m,mujoco.mjtObj.mjOBJ_KEY,pose))
        names=meta['joint_names'].split(',')
        joints=[mujoco.mj_name2id(m,mujoco.mjtObj.mjOBJ_JOINT,n) for n in names]
        if min(joints)<0:raise ValueError('Joint contract mismatch')
        qp=m.jnt_qposadr[joints];qv=m.jnt_dofadr[joints]
        actuator_ids=[int(np.flatnonzero(m.actuator_trnid[:,0]==j)[0]) for j in joints]
        defaults=np.array([float(v) for v in meta['default_joint_pos'].split(',')])
        scale=float(meta['action_scale']);previous=np.zeros(14)
        trunk=mujoco.mj_name2id(m,mujoco.mjtObj.mjOBJ_BODY,'trunk_base')
        gyro=mujoco.mj_name2id(m,mujoco.mjtObj.mjOBJ_SENSOR,'imu_ang_vel')
        mujoco.mj_forward(m,d);rows=[]
        for tick in range(500):
            R=d.xmat[trunk].reshape(3,3)
            obs=np.r_[d.sensordata[m.sensor_adr[gyro]:m.sensor_adr[gyro]+3],
                      R.T@np.array([0,0,-1]),d.qpos[qp]-defaults,d.qvel[qv],previous,np.zeros(13)].astype(np.float32)
            action=policy.run(None,{policy.get_inputs()[0].name:obs.reshape(1,61)})[0][0]
            if not np.isfinite(action).all():raise ValueError('Nonfinite action')
            previous=action;d.ctrl[actuator_ids]=defaults+action*scale
            for _ in range(4):mujoco.mj_step(m,d)
            R=d.xmat[trunk].reshape(3,3)
            rows.append({'time':float(d.time),'trunkZ':float(d.xpos[trunk,2]),
                         'tiltDeg':float(np.degrees(np.arccos(np.clip(R[2,2],-1,1)))),
                         'qpos':d.qpos.tolist(),'action':action.tolist()})
        case={'policy':policy_name,'initialPose':pose,'simSeconds':10,
              'finalZ':rows[-1]['trunkZ'],'finalTiltDeg':rows[-1]['tiltDeg'],
              'lastSecondMaxTiltDeg':max(r['tiltDeg'] for r in rows[-50:])}
        summary['cases'].append(case)
        (args.output/f'{policy_name}-{pose}.json').write_text(json.dumps(rows),encoding='utf-8')
(args.output/'summary.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
print(json.dumps(summary,indent=2))
