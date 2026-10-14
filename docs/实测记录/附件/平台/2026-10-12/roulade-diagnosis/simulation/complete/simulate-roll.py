import sys,importlib.util,json,hashlib,math
from pathlib import Path
from collections import deque
import numpy as np,mujoco,onnxruntime as ort
root=Path(__file__).resolve().parent
reference=Path('/home/leiwu30/microduck-training/getup-contact-isolation-20261010')
spec=importlib.util.spec_from_file_location('cpu_roll_helpers',root/'evaluate_skills_cpu.py');E=importlib.util.module_from_spec(spec);spec.loader.exec_module(E);E.ROOT=reference
meta=json.loads((reference/'policies/getup.metadata.json').read_text())
contract=json.loads((root/'contract.json').read_text())
limits=json.loads((root/'limits.json').read_text())
model,bam,ground=E.build_model(meta)
names=contract['runtime_joint_order'];home=np.array(contract['runtime_home_rad'])
assert meta['actuatorParamsSha256']==contract['actuators'][0]['parameter_file']['sha256']
assert hashlib.sha256((root/'roulade.onnx').read_bytes()).hexdigest()=='8fdaf12c7478a8314a63647a466bfa34ba1f15a7d9a5cb3b6ffa242719666168'
mass_differences={b['name']:float(model.body(b['name']).mass[0])-b['mass_kg'] for b in contract['model']['bodies'] if b['name']!='world'}
assert max(abs(x) for x in mass_differences.values())<1e-7,mass_differences
original={j['name']:j['range'] for j in contract['model']['joints'] if j['name'] in names}
mechanical={j['name']:[j['min_rad'],j['max_rad']] for j in limits['joints'] if j['name'] in names}
trunk=model.body('trunk_base').id
joint_ids=np.array([model.joint(n).id for n in names]);qadr=model.jnt_qposadr[joint_ids];vadr=model.jnt_dofadr[joint_ids]
opt=ort.SessionOptions();opt.intra_op_num_threads=opt.inter_op_num_threads=1
session=ort.InferenceSession(str(root/'roulade.onnx'),sess_options=opt,providers=['CPUExecutionProvider'])
assert session.get_inputs()[0].shape==[1,61]
def first_frame(file):
 d=json.loads((root/file).read_text());return next(f for f in d['frames'] if f.get('kind')=='xgoduck_roulade' and f.get('type')=='inference')
starts=[first_frame('before.json'),first_frame('after.json')]
default_friction=model.dof_frictionloss.copy();default_damping=model.dof_damping.copy()
results=[]
for initial,frame in enumerate(starts):
 for range_name,ranges in [('training',original),('mechanical',mechanical)]:
  for clip_target in [False,True]:
   label=f'start{initial}-{range_name}-'+('clipped' if clip_target else 'unbounded')
   model.jnt_range[joint_ids]=np.array([ranges[n] for n in names]);model.dof_frictionloss[:]=default_friction;model.dof_damping[:]=default_damping
   bam.actuator.kp=5.;bam.actuator.vin=7.4
   data=mujoco.MjData(model);controller=E.MujocoController(model,data,bam)
   obs=np.array(frame['observation']);g=obs[3:6];pitch=math.asin(np.clip(g[0],-1,1));roll=math.atan2(-g[1],-g[2])
   data.qpos[:7]=[0,0,.12,*E.quaternion(roll,pitch,0.)];data.qpos[qadr]=obs[6:20]+home
   data.qvel[vadr]=obs[20:34];data.qvel[3:6]=obs[:3]
   mujoco.mj_forward(model,data);data.qpos[2]+=.003-ground.lowest_mesh_z(model,data);mujoco.mj_forward(model,data)
   controller.q_target[:]=data.qpos[qadr];bam.actuator.q_target_smooth=data.qpos[qadr].copy()
   delay_steps=E.command_delay_steps(meta,'runtime');history=deque([data.qpos[qadr].copy() for _ in range(delay_steps+1)],maxlen=delay_steps+1)
   previous=obs[34:48].copy();delayed_velocity=obs[20:34].copy();forward=0.;trace=[];qpositions=[]
   for tick in range(150):
    gyro,gravity=E.root_body_imu_observation(model,data,trunk)
    observation=np.concatenate([gyro,gravity,data.qpos[qadr]-home,delayed_velocity,previous,np.zeros(13)]).astype(np.float32)
    delayed_velocity=data.qvel[vadr].copy()
    action=session.run(None,{session.get_inputs()[0].name:observation[None]})[0][0];previous=action.copy()
    raw=home+action;target=np.clip(raw,model.jnt_range[joint_ids,0],model.jnt_range[joint_ids,1]) if clip_target else raw
    forward+=float(gyro[1])*.02
    trace.append({'t':tick*.02,'tilt':math.degrees(math.acos(np.clip(-gravity[2],-1,1))),'signedPitchIntegralDegrees':math.degrees(forward),'clippedJoints':int(np.count_nonzero(abs(raw-target)>1e-6))})
    qpositions.append(data.qpos.copy())
    for _ in range(round(.02/model.opt.timestep)):
     history.append(target.copy());controller.q_target[:]=history[0];controller.update();mujoco.mj_step(model,data)
    assert np.isfinite(data.qpos).all()
   outcome={'label':label,'at1sec':trace[49],'at2sec':trace[99],'at3sec':trace[-1],'maxSignedPitchIntegralDegrees':max(f['signedPitchIntegralDegrees'] for f in trace),'maxTilt':max(f['tilt'] for f in trace)}
   results.append(outcome);(root/(label+'.json')).write_text(json.dumps({'summary':outcome,'frames':trace},indent=2));np.savez_compressed(root/(label+'.npz'),qpos=qpositions)
   print(json.dumps(outcome),flush=True)
summary={'training':False,'gpuAccess':False,'hardwareAccess':False,'frictionRevision':'dof-id-friction-force-v1','bodyMassMaxDifference':max(abs(x) for x in mass_differences.values()),'constantSimVoltage':7.4,'actorFrozen':True,'sameInitialQposAndQvelWithinEachStart':True,'diagnosticOnly':True,'limitations':['estimated base height from floor clearance','base translation velocity unavailable; set zero','no real transient voltage replay','replica CPU contact assembly not full training/hardware contact equivalence','signed pitch integral is a diagnostic, not certified full-roll success'],'results':results}
(root/'summary.json').write_text(json.dumps(summary,indent=2));(root/'job.exit').write_text('0')
