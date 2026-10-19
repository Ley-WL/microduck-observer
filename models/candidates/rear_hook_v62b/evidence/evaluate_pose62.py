"""Closed-loop native contract: quantized encoder sensors/targets, lagged speed,
zero action histories at walk handoff, deployed FK clock, and true contact score.
"""
import argparse,json,random
from dataclasses import dataclass
from pathlib import Path
import numpy as np,torch,onnxruntime as ort
from tensordict import TensorDict
from rsl_rl.models import MLPModel
import mjlab_microduck.tasks
from mjlab.envs import ManagerBasedRlEnv
from contact_truth60 import make_cfg,measurements
from rear_cycle_env import OneShotCommand,OneShotCommandCfg
from rear_encoder_v32 import restore as restore_original
from rear_transfer_residual53 import restore as restore_transfer
from speed_reporter57 import VelocityReporter
from termination_capture58 import TerminationCapture
from rear_runtime_proxy import Geometry
from pose_entry_clock62 import PoseEntryClock as TransferClock

class ExternalCommand(OneShotCommand):
 def compute(self,dt):self._sync()
@dataclass(kw_only=True)
class ExternalCommandCfg(OneShotCommandCfg):
 def build(self,env):return ExternalCommand(self,env)

p=argparse.ArgumentParser();p.add_argument('--checkpoint',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--worlds',type=int,default=32);p.add_argument('--seed',type=int,default=108401);p.add_argument('--scenario',choices=['normal','cancel-entry','cancel-hold'],default='normal');p.add_argument('--walk-scale',type=float,required=True);p.add_argument('--model-kind',choices=['original','transfer53'],required=True);p.add_argument('--speed-profile',choices=['legacy20ms','delay40ms','window56'],required=True);p.add_argument('--enter-timeout',type=float,default=3.);a=p.parse_args();assert a.enter_timeout>=3.;a.output.mkdir(parents=True,exist_ok=False);restore=restore_original if a.model_kind=='original' else restore_transfer
torch.manual_seed(a.seed);np.random.seed(a.seed);random.seed(a.seed)
cfg=make_cfg(a.worlds,play=True);cfg.episode_length_s=25.;cfg.terminations.pop('cycle_finished');cfg.commands['twist']=ExternalCommandCfg(**vars(cfg.commands['twist']))
cfg.actions['joint_pos'].alpha=0.
env=ManagerBasedRlEnv(cfg,device='cuda:0')
try:
 with torch.inference_mode():
  obs,_=env.reset(seed=a.seed);actor=MLPModel(TensorDict({'actor':torch.zeros(1,61,device=env.device)},batch_size=[1]),{'actor':['actor']},'actor',14,hidden_dims=(512,256,128),activation='elu',obs_normalization=True).to(env.device);restore(actor,torch.load(a.checkpoint,map_location=env.device,weights_only=False)['actor_state_dict']);actor.eval()
  r=env.scene['robot'];home=r.data.default_joint_pos;geometry=Geometry('inputs/native-geometry.json',env.device)
  meta=json.loads(Path('inputs/device-calibration.json').read_text());order=[20,21,22,23,24,30,31,32,33,10,11,12,13,14]
  reference=torch.tensor([meta['joints']['references'][str(i)] for i in order],device=env.device,dtype=torch.float64);direction=torch.tensor([meta['joints']['directions'].get(str(i),-1) for i in order],device=env.device,dtype=torch.float64)
  reporter=VelocityReporter(a.speed_profile);command=env.command_manager.get_term('twist');command.clock=TransferClock(a.worlds,env.device);command.clock.enter_limit=round(a.enter_timeout/.02);command.trigger();last=torch.zeros(a.worlds,14,device=env.device);filtered=last.clone();velocity=r.data.joint_vel.clone();alive=torch.ones(a.worlds,dtype=torch.bool,device=env.device);cancelled=torch.zeros_like(alive)
  walk=ort.InferenceSession('inputs/walk.onnx',providers=['CPUExecutionProvider']);walkname=walk.get_inputs()[0].name
  walking=torch.zeros_like(alive);walk_initialized=torch.zeros_like(alive);exit_checked=torch.zeros_like(alive);exit_ok=torch.zeros_like(alive)
  weights=torch.tensor([.7]*5+[.5]*4+[.7]*5,device=env.device)
  mechanical=torch.tensor([[-25,30],[-22,22],[-90,90],[-90,90],[-70,70],[-90,70],[-90,90],[-170,170],[-20,20],[-30,25],[-22,22],[-90,90],[-90,90],[-70,70]],device=env.device)*torch.pi/180
  sole_heights=[];visual_heights=[];frames=[];qpos=[];inputs=[];actions=[];stages=[];held=[];native=[];walk_masks=[]
  initial=torch.cat((r.data.root_link_pos_w,r.data.root_link_quat_w,r.data.joint_pos),-1).cpu().numpy()
  capture=TerminationCapture(env,lambda:dict(stage=command.clock.stage,progress=command.clock.progress,observation=x,raw=value))
  for tick in range(550 if a.enter_timeout==3 else 650):
   capture.tick=tick
   done=command.clock.done&~exit_checked
   exit_checked|=done;exit_ok|=done&(command.clock.stand_run>=48);alive&=~(done&~exit_ok)
   new_walk=done&exit_ok;walking|=new_walk;last[new_walk]=0.;filtered[new_walk]=0.
   q=r.data.joint_pos;enc=(reference+q.double()*4096/torch.pi/2*direction).round();measured=((enc-reference)*torch.pi*2/4096*direction).float()
   measured_velocity=reporter.sample(measured,r.data.joint_vel)
   x=torch.zeros(a.worlds,61,device=env.device);x[:,:3]=r.data.root_link_ang_vel_b;x[:,3:6]=r.data.projected_gravity_b;x[:,6:20]=measured-home;x[:,20:34]=measured_velocity;x[:,34:48]=last
   if a.scenario=='cancel-entry' and tick==20:command.cancel();cancelled[:]=True
   if a.scenario=='cancel-hold':
    ids=((command.clock.stage==command.clock.HOLD)&(command.clock.hold_ticks>=50)&~cancelled).nonzero().flatten()
    command.cancel(ids);cancelled[ids]=True
   pose,standing,clearance,rear,speed=geometry.sample(x,home)
   transfer_good=(clearance.abs()<.01)&(x[:,:3].norm(dim=-1)<.3)&(speed<.02)&(-x[:,5]>.8660254)
   command.clock.step(pose,standing,transfer_good);command._sync();x[:,48]=command.clock.enabled.float();x[:,49]=command.clock.progress;x[:,50]=.035
   x[walking,48:61]=0.
   value=actor(TensorDict({'actor':x},batch_size=[a.worlds]));next_filtered=filtered*.45+value*.55
   if walking.any():
    wr=torch.from_numpy(np.concatenate([walk.run(None,{walkname:row[None]})[0] for row in x[walking].cpu().numpy()])).to(env.device)
    value[walking]=wr
    walk_target=wr*a.walk_scale
    next_filtered[walking]=torch.where(walk_initialized[walking,None],filtered[walking]*(1-weights)+walk_target*weights,walk_target)
    walk_initialized|=walking
   filtered=next_filtered;targets=(home+filtered).double()
   targets[walking]=targets[walking].clamp(mechanical[:,0],mechanical[:,1])
   goals=(reference+targets*4096/(torch.pi*2)*direction).round().clamp(0,4095);sent=((goals-reference)*torch.pi*2/4096*direction).float()
   # alpha0 here: rear EMA was already applied above, exactly once.
   obs,_,term,trunc,_=env.step(sent-home);last=value;m=measurements(env);alive&=~(term|trunc|m['body']|~torch.isfinite(x).all(-1))
   sole_heights.append(m['sole_h'].cpu().numpy());visual_heights.append(m['visual_h'].cpu().numpy());frames.append(torch.stack((m['h'][:,0],m['h'][:,1],m['tilt'],m['pose'].float(),alive.float(),m['rear'],m['lin'],m['ang'],m['slip'],m['head'],m['stand'].float()),-1).cpu().numpy());qpos.append(torch.cat((r.data.root_link_pos_w,r.data.root_link_quat_w,r.data.joint_pos),-1).cpu().numpy());inputs.append(x.cpu().numpy());actions.append(value.cpu().numpy());stages.append(command.clock.stage.cpu().numpy().copy());held.append(command.clock.hold_ticks.cpu().numpy().copy());native.append(torch.stack((pose.float(),standing.float(),clearance,rear,speed),-1).cpu().numpy());walk_masks.append(walking.cpu().numpy().copy())
  f=np.stack(frames);stage=np.stack(stages);good=(f[:,:,3]>0)&(stage==2)&(f[:,:,4]>0);run=np.zeros(a.worlds,int);best=run.copy()
  for row in good:run=np.where(row,run+1,0);best=np.maximum(best,run)
  returned=(f[-50:,:,10].sum(0)>=48)&(f[-1,:,4]>0)
  if a.scenario=='cancel-hold':returned &= cancelled.cpu().numpy()
  proxy_stand=exit_ok.cpu().numpy();cycle=(best>=150)&returned&proxy_stand
  result=dict(seed=a.seed,scenario=a.scenario,worlds=a.worlds,alive=int(alive.sum()),fullCycles=int(cycle.sum()),returns=int((returned&proxy_stand).sum()),trueReturns=int(returned.sum()),proxyReturns=int(proxy_stand.sum()),enteredHold=int((np.stack(held).max(0)>0).sum()),cancelActuallyDuringHold=int(cancelled.sum()) if a.scenario=='cancel-hold' else None,fullCycleByWorld=cycle.tolist(),fullReturnByWorld=(returned&proxy_stand).tolist(),longestHoldTicks=best.tolist(),accepted=False,nativeContract=True,contactTruthRequired=True,hardwareMotion=False)
  result.update(poseEntryConsecutiveReadyTicks=15,holdTicksUnchanged=150,developmentReplayOfKnownSeeds=True,footShellsExcludedFromBodyOnly=True,wholeFootClearanceIncludesVisualShell=True,physicsCollisionsUnchanged=True,entryTimeoutSeconds=a.enter_timeout,observationSeconds=11 if a.enter_timeout==3 else 13,terminationBeforeResetCaptured=True,speedProfile=a.speed_profile,modelKind=a.model_kind,simulationSensorHypothesisOnly=True,initialReporterHistoryBackExtrapolated=True,returnedToActualWalkingActor=int(walking.sum()),walkingActionScale=a.walk_scale,walkingVoltageScale='nominal1; rear scale1 by contract',lateStandingCannotRecoverFailedExit=True,experimentalSettleBeforeLift=True,settleTransferConsecutiveTicks=15,transferGatePassed=int(command.clock.transfer_passed.sum()))
  capture.save(a.output/'termination-capture.json');np.savez_compressed(a.output/'records.npz',sole_heights=np.stack(sole_heights),visual_heights=np.stack(visual_heights),initial=initial,frames=f,qpos=np.stack(qpos),observation=np.stack(inputs),delivered_action=np.stack(actions),stage=stage,hold_ticks=np.stack(held),proxy=np.stack(native),walking=np.stack(walk_masks));(a.output/'summary.json').write_text(json.dumps(result,indent=2));print('RESULT '+json.dumps(result),flush=True)
finally:env.close()
