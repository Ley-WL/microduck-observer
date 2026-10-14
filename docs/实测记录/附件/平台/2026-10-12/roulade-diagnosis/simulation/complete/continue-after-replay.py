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
assert meta['actuatorParamsSha256']==contract['model']['actuators'][0]['parameter_file']['sha256']
assert hashlib.sha256((root/'roulade.onnx').read_bytes()).hexdigest()=='8fdaf12c7478a8314a63647a466bfa34ba1f15a7d9a5cb3b6ffa242719666168'
mass_differences={b['name']:float(model.body(b['name']).mass[0])-b['mass_kg'] for b in contract['model']['bodies'] if b['name']!='world'}
# The reusable CPU scene predates donor ballast; align named body fields to the actual rollout contract.
for b in contract['model']['bodies']:
 if b['name']=='world':continue
 i=model.body(b['name']).id
 model.body_mass[i]=b['mass_kg'];model.body_inertia[i]=b['inertia'];model.body_ipos[i]=b['ipos'];model.body_iquat[i]=b['iquat'];model.body_pos[i]=b['pos'];model.body_quat[i]=b['quat']
mujoco.mj_setConst(model,mujoco.MjData(model))
assert max(abs(float(model.body(b['name']).mass[0])-b['mass_kg']) for b in contract['model']['bodies'] if b['name']!='world')<1e-12
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
for initial,filename in enumerate(['before.json','after.json']):
 actual=json.loads((root/filename).read_text());frames=[f for f in actual['frames'] if f.get('kind')=='xgoduck_roulade' and f.get('type')=='inference'];frame=frames[0]
 for voltage_mode in ['constant7.4','recordedMean']:
  model.jnt_range[joint_ids]=np.array([mechanical[n] for n in names]);model.dof_frictionloss[:]=default_friction;model.dof_damping[:]=default_damping
  bam.actuator.kp=5.;bam.actuator.vin=7.4
  data=mujoco.MjData(model);controller=E.MujocoController(bam,names,model,data,vin_drop_gain=0.,vin_min=4.)
  obs=np.array(frame['observation']);g=obs[3:6];pitch=math.asin(np.clip(g[0],-1,1));roll=math.atan2(-g[1],-g[2])
  data.qpos[:7]=[0,0,.12,*E.quaternion(roll,pitch,0.)];data.qpos[qadr]=obs[6:20]+home;data.qvel[vadr]=obs[20:34];data.qvel[3:6]=obs[:3]
  mujoco.mj_forward(model,data);data.qpos[2]+=.003-ground.lowest_mesh_z(model,data);mujoco.mj_forward(model,data)
  controller.q_target[:]=data.qpos[qadr];bam.actuator.q_target_smooth=data.qpos[qadr].copy()
  delay_steps=E.command_delay_steps(meta,'runtime');history=deque([data.qpos[qadr].copy() for _ in range(delay_steps+1)],maxlen=delay_steps+1)
  forward=0.;trace=[];sim_time=0.
  for tick,f in enumerate(frames):
   gyro,gravity=E.root_body_imu_observation(model,data,trunk)
   period=frames[tick+1]['mono']-f['mono'] if tick+1<len(frames) else .02
   period=max(.005,min(.1,period));forward+=float(gyro[1])*period
   real_tilt=math.degrees(math.acos(np.clip(-f['observation'][5],-1,1)))
   volts=[r['voltage'] for id,r in f['feedbackBeforeWrite'].items() if id!='34'];v=float(np.mean(volts))
   trace.append({'progress':f['skillProgressSeconds'],'simTilt':math.degrees(math.acos(np.clip(-gravity[2],-1,1))),'realTilt':real_tilt,'simSignedPitchIntegralDegrees':math.degrees(forward),'jointRmsErrorDegrees':float(np.sqrt(np.mean((np.degrees(data.qpos[qadr])-np.degrees(np.array(f['observation'][6:20])+home))**2))),'vin':v if voltage_mode=='recordedMean' else 7.4})
   if voltage_mode=='recordedMean':bam.actuator.vin=v
   target=np.deg2rad(f['sentTargetDegrees'])
   substeps=max(1,round((sim_time+period)/model.opt.timestep)-round(sim_time/model.opt.timestep));sim_time+=period
   for _ in range(substeps):history.append(target.copy());controller.q_target[:]=history[0];controller.update();mujoco.mj_step(model,data)
  summary={'start':initial,'voltageMode':voltage_mode,'frames':len(trace),'final':trace[-1],'maxSimPitchIntegral':max(f['simSignedPitchIntegralDegrees'] for f in trace),'maxSimTilt':max(f['simTilt'] for f in trace)}
  replay_end=dict(summary)
  previous=np.array(frames[-1]['rawActionRadians']);delayed_velocity=np.array(frames[-1]['observation'][20:34]);continuation=[]
  for extra in range(100):
   gyro,gravity=E.root_body_imu_observation(model,data,trunk)
   observation=np.concatenate([gyro,gravity,data.qpos[qadr]-home,delayed_velocity,previous,np.zeros(13)]).astype(np.float32);delayed_velocity=data.qvel[vadr].copy()
   action=session.run(None,{session.get_inputs()[0].name:observation[None]})[0][0];previous=action.copy()
   target=np.clip(home+action,model.jnt_range[joint_ids,0],model.jnt_range[joint_ids,1]);forward+=float(gyro[1])*.02
   continuation.append({'extraSeconds':extra*.02,'tilt':math.degrees(math.acos(np.clip(-gravity[2],-1,1))),'pitchIntegral':math.degrees(forward)})
   for _ in range(round(.02/model.opt.timestep)):history.append(target.copy());controller.q_target[:]=history[0];controller.update();mujoco.mj_step(model,data)
  summary={'replay':replay_end,'continuationFinal':continuation[-1],'continuationMaxPitch':max(f['pitchIntegral'] for f in continuation)}
  results.append(summary);(root/f'continue-{initial}-{voltage_mode}.json').write_text(json.dumps({'summary':summary,'trace':trace,'continuation':continuation},indent=2));print(json.dumps(summary),flush=True)
(root/'continuation-summary.json').write_text(json.dumps({'diagnosticOnly':True,'hardwareMotionAdded':False,'training':False,'gpuAccess':False,'targetStream':'recorded quantized sent angles','timeStream':'recorded frame times','voltageReplay':'joint mean scalar approximation; no power circuit model','results':results},indent=2))
