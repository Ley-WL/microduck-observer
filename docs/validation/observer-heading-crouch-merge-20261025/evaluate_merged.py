"""Actual ONNX swizzle -> official crouch phase -> swizzle, never HOME/reset mid-skill."""
import argparse,hashlib,json,math
from pathlib import Path
import numpy as np
import onnxruntime as ort
import torch
from mjlab.envs import ManagerBasedRlEnv
from mjlab_microduck.tasks.roller_crouch_observer_hd1910 import make_cfg,STAND_POSE,BOARD_CROUCH_POSE
from mjlab_microduck.tasks.getup_mechanical_contract import NAMES


def main(args):
    out=Path(args.output);out.mkdir(parents=True,exist_ok=False)
    options=ort.SessionOptions();options.intra_op_num_threads=1
    def load(path):return ort.InferenceSession(str(path),sess_options=options,providers=['CPUExecutionProvider'])
    crouch=load(args.policy);swizzle_path=Path('/CodeRepo/Code/leiwu30/hd1910-roller-crouch-official-20261025/validation-models/swizzle_hd1910.onnx')
    swizzle=load(swizzle_path)
    assert hashlib.sha256(swizzle_path.read_bytes()).hexdigest()=='539e6b2e7e4c7603ddd35ef27bc06742b292f9f3eede567f82111c0975375160'
    cases=[('stationary',0.,None),('forward_coast',.35,None),('reverse_coast',-.35,None),
           ('swizzle_forward',0.,[.4,0.,0.]),('swizzle_reverse',0.,[-.4,0.,0.]),
           ('swizzle_turn_left',0.,[.4,0.,.1]),('swizzle_turn_right',0.,[.4,0.,-.1])]
    result=[]
    for seed in [25102711,25102729]:
        for name,velocity,drive in cases:
            cfg=make_cfg(play=True);cfg.seed=seed;cfg.scene.num_envs=args.envs
            cfg.episode_length_s=30.;cfg.curriculum.clear();cfg.observations['actor'].enable_corruption=False
            if not args.stress:
                for key in ('push_robot','base_com','randomize_com','randomize_head_com','randomize_mass_inertia','randomize_armature','randomize_joint_friction','encoder_bias','imu_orientation'):
                    cfg.events.pop(key,None)
                cfg.observations['actor'].terms['joint_pos'].params['biased']=False
                for actuator in cfg.scene.entities['robot'].articulation.actuators:
                    actuator.vin_range=(7.4,7.4);actuator.vin_drop_gain_range=(0.,0.)
            cfg.events['reset_base'].params['velocity_range']={'x':(velocity,velocity)}
            cfg.events['reset_base'].params['pose_range']['yaw']=(0.,0.)
            env=ManagerBasedRlEnv(cfg,device='cuda:0')
            try:
                obs,_=env.reset(seed=seed);robot=env.scene['robot']
                ids=[int(robot.find_joints([n])[0][0]) for n in NAMES]
                home=np.asarray([STAND_POSE[n] for n in NAMES]);low=np.asarray([BOARD_CROUCH_POSE[n] for n in NAMES])
                legs=[0,1,2,3,4,9,10,11,12,13]
                pre=3. if drive is not None else 0.;end=pre+3.5;seconds=end+3.
                bad=np.zeros(args.envs,bool);down_run=np.zeros(args.envs);down_best=np.zeros(args.envs)
                up_run=np.zeros(args.envs);up_best=np.zeros(args.envs);lowest=np.full(args.envs,np.inf)
                max_tilt=np.zeros(args.envs);records=[];qposes=[];entry=None;entry_velocity=None;heading_target=None;last_actor=None;was_turning=False
                for tick in range(round(seconds/.02)):
                    t=tick*.02
                    active=pre<=t<end;net=crouch if active else swizzle
                    if last_actor is None or active!=last_actor:
                        heading_target=None;was_turning=False;dt=0.
                    else:dt=.02
                    last_actor=active
                    if active:
                        command=np.tile([math.cos(math.tau*(t-pre)/5.),math.sin(math.tau*(t-pre)/5.),0.],(args.envs,1))
                    else:
                        # Exact fresh-input branch of merged Rust Heading::step.
                        # Returning from crouch starts zero-input roller balance.
                        wire=drive if t<pre and drive is not None else [0.,0.,0.]
                        quat=robot.data.root_link_quat_w.cpu().numpy();w,x,y,z=quat.T
                        yaw=np.arctan2(2*(w*z+x*y),1-2*(y*y+z*z))
                        wrap=lambda angle:(angle+math.pi)%(2*math.pi)-math.pi
                        turn=float(np.clip(wire[2],-.25,.25));turning=abs(turn)>1e-6
                        if heading_target is None:heading_target=yaw.copy()
                        if was_turning and not turning:heading_target=yaw.copy()
                        heading_target=wrap(heading_target+turn*2*min(dt,.05))
                        error=np.clip(wrap(heading_target-yaw),-.25,.25)
                        heading_target=wrap(yaw+error);was_turning=turning
                        command=np.column_stack([np.full(args.envs,wire[0]),np.zeros(args.envs),error])
                    if tick==round(pre/.02):
                        entry=robot.data.root_link_pos_w[:,2].cpu().numpy().copy()
                        entry_velocity=robot.data.root_link_lin_vel_b[:,:2].cpu().numpy().copy()
                    rows=obs['actor'].cpu().numpy().copy();rows[:,48:51]=np.asarray(command,np.float32);rows[:,51:61]=0.
                    action=np.concatenate([net.run(None,{net.get_inputs()[0].name:r[None]})[0] for r in rows])
                    assert action.shape==(args.envs,14) and np.isfinite(action).all()
                    with torch.no_grad():obs,_,done,timeout,_=env.step(torch.as_tensor(action,device=env.device))
                    q=robot.data.joint_pos[:,ids].cpu().numpy();tilt=np.rad2deg(torch.acos((-robot.data.projected_gravity_b[:,2]).clamp(-1,1)).cpu().numpy())
                    bad|=(done|timeout).cpu().numpy()|(tilt>=45.);max_tilt=np.maximum(max_tilt,tilt)
                    height=robot.data.root_link_pos_w[:,2].cpu().numpy();velocity_now=robot.data.root_link_lin_vel_b[:,:2].cpu().numpy()
                    gyro=robot.data.root_link_ang_vel_b.cpu().numpy();low_error=np.abs(q[:,legs]-low[legs]).mean(1)
                    home_error=np.abs(q[:,legs]-home[legs]).mean(1)
                    if pre+.5<=t<pre+2.5:
                        down=(low_error<.45)&((q[:,3]-q[:,12])*.5>1.)&(tilt<35.)&~bad
                        down_run=np.where(down,down_run+.02,0.);down_best=np.maximum(down_best,down_run);lowest=np.minimum(lowest,height)
                    if t>=pre+3.:
                        up=(home_error<.3)&(tilt<20.)&(np.linalg.norm(gyro,axis=1)<1.)&~bad
                        up_run=np.where(up,up_run+.02,0.);up_best=np.maximum(up_best,up_run)
                    records.append(np.column_stack([height,tilt,velocity_now,low_error,home_error,gyro]))
                    if tick%2==0:qposes.append(env.sim.data.qpos[0].cpu().numpy().copy())
                survived=~bad;lowered=(entry-lowest)>.015
                success=survived&(down_best>=.4)&(up_best>=.3)&lowered
                case=dict(case=name,seed=seed,worlds=args.envs,survived=int(survived.sum()),completed=int(success.sum()),
                    crouched=int((down_best>=.4).sum()),stoodAgain=int((up_best>=.3).sum()),
                    entryVelocity=entry_velocity.tolist(),heightDropMeters=(entry-lowest).tolist(),
                    crouchStableSeconds=down_best.tolist(),standStableSeconds=up_best.tolist(),maxTiltDegrees=max_tilt.tolist(),
                    success=success.tolist(),noStateResetAtSwitch=True,rawActionHistoryRetained=True)
                result.append(case)
                np.savez_compressed(out/f'{name}-{seed}.npz',trace=np.asarray(records),qpos=np.asarray(qposes),origin=env.scene.env_origins[0].cpu().numpy(),dt=.02)
                print(json.dumps({k:case[k] for k in ['case','seed','worlds','survived','completed','crouched','stoodAgain']}),flush=True)
            finally:env.close()
    report=dict(policySha256=hashlib.sha256(Path(args.policy).read_bytes()).hexdigest(),stress=args.stress,
        mergedHeadingController=True,mergeBase='44556ec',postCrouchCommand='zero-input heading hold; target reset on actor switch',phasePeriod=5.,endPhase=.7,secondsOfCrouchPolicy=3.5,hardwareAccess=False,screenOnly=True,needsReview=True,
        worlds=sum(r['worlds'] for r in result),completed=sum(r['completed'] for r in result),cases=result,
        criterion='no fall; >15mm height drop; leg-pose MAE<.45rad and knee flexion>1rad stable .4s; return pose MAE<.3rad/tilt<20deg/gyro<1 stable .3s')
    (out/'results.json').write_text(json.dumps(report,indent=2),encoding='utf-8')


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--policy',required=True);p.add_argument('--output',required=True)
    p.add_argument('--envs',type=int,default=16);p.add_argument('--stress',action='store_true');main(p.parse_args())
