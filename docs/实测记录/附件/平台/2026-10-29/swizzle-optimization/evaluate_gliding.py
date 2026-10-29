"""Frozen deployed ONNX: stick grid and closed-loop heading diagnostic."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import numpy as np
import onnxruntime as ort
import torch
from mjlab.envs import ManagerBasedRlEnv
from mjlab_microduck.tasks.swizzle_observer_hd1910 import make_cfg


def wrap(x):
    return (x + math.pi) % (2 * math.pi) - math.pi


def stick(x, y):
    if math.hypot(x, y) < .15:
        return [0., 0., 0.]
    scale = max(1., math.hypot(x, y))
    return [-y / scale * .4, 0., -x / scale * .25]


def yaw_of(q):
    w, x, y, z = q.unbind(-1)
    return torch.atan2(2 * (w*z + x*y), 1 - 2 * (y*y + z*z))


def run(args, case_defs=None):
    source = Path(args.source)
    policy = Path(args.policy)
    policy_sha = hashlib.sha256(policy.read_bytes()).hexdigest()
    expected = json.loads((source/'evidence/expected-source.json').read_text())
    mismatches = [p for p, h in expected.items() if hashlib.sha256((source/p).read_bytes()).hexdigest() != h]
    assert not mismatches, mismatches
    out = Path(args.output); out.mkdir(parents=True, exist_ok=False)
    cases = [dict(id=f'x{x:+.1f}_y{y:+.1f}', stick=[x,y], command=stick(x,y), mode='current')
             for y in [-1., -.5, 0., .5, 1.] for x in [-1., -.5, 0., .5, 1.]]
    cases += [dict(id=f'hold_y{y:+.1f}', stick=[0.,y], command=stick(0.,y), mode='heading_hold')
              for y in [-1., -.5, .5, 1.]]
    if case_defs is not None:cases=case_defs
    # Two independent seeds, eight replicates each. Different cases are not
    # guaranteed identical randomized starting states; do not claim paired A/B.
    all_rows=[]; render_records=[]; all_traces=[]
    opts=ort.SessionOptions();opts.intra_op_num_threads=1
    net=ort.InferenceSession(str(policy),sess_options=opts,providers=['CPUExecutionProvider'])
    assert net.get_inputs()[0].shape[-1] == 61
    for seed in [291601,291607]:
        cfg=make_cfg(play=True);cfg.seed=seed;cfg.scene.num_envs=len(cases)*16
        cfg.episode_length_s=30.;cfg.curriculum.clear();cfg.observations['actor'].enable_corruption=False
        for key in ('push_robot','randomize_com','randomize_head_com','randomize_mass_inertia','randomize_armature','randomize_joint_friction','encoder_bias','imu_orientation'):
            cfg.events.pop(key,None)
        cfg.observations['actor'].terms['joint_pos'].params['biased']=False
        for actuator in cfg.scene.entities['robot'].articulation.actuators:
            actuator.vin_range=(7.4,7.4);actuator.vin_drop_gain_range=(0.,0.)
        env=ManagerBasedRlEnv(cfg,device='cuda:0')
        try:
            obs,_=env.reset(seed=seed);robot=env.scene['robot'];term=env.command_manager.get_term('twist')
            n=cfg.scene.num_envs;bad=np.zeros(n,dtype=bool);first_failure=np.full(n,np.nan)
            base=torch.tensor([c['command'] for c in cases],device=env.device).repeat_interleave(16,0)
            gain=torch.tensor([c.get('gain',2.) for c in cases],device=env.device).repeat_interleave(16)
            cap=torch.tensor([c.get('cap',.5) for c in cases],device=env.device).repeat_interleave(16)
            kick=torch.tensor([c.get('kick',0.) for c in cases],device=env.device).repeat_interleave(16)
            schedule=torch.tensor([c['schedule'] for c in cases],device=env.device).repeat_interleave(16,0)
            drive=torch.zeros(n,device=env.device)
            histories=[];qposes=[];origins=env.scene.env_origins.cpu().numpy()
            yaw_anchor=None
            for tick in range(800):
                yaw=yaw_of(robot.data.root_link_quat_w)
                if tick==100:yaw_anchor=yaw.clone()
                cmd=schedule[:,tick,:].clone()
                wire=cmd.clone()
                desired=cmd[:,0].clone()
                if args.profile=='suppress':
                    u=((desired.abs()-.15)/.25).clamp(0.,1.)
                    desired=torch.where(cmd[:,2].abs()>1e-6,torch.sign(desired)*.4*u*u*(3.-2.*u),desired)
                # Acceleration/reversal ramps; release and stale zero remain immediate.
                drive=drive+(desired-drive).clamp(-.016,.016)
                drive=torch.where(desired==0.,0.,drive)
                cmd[:,0]=drive
                low=.2 if args.profile in ('plateau','suppress') else 0.
                u=((drive.abs()-low)/(.4-low)).clamp(0.,1.)
                blend=u*u*(3.-2.*u)
                slew=torch.sign(cmd[:,2])*(.9*(1.-blend)+cmd[:,2].abs()*2.*blend)
                slew=torch.where(cmd[:,2].abs()>1e-6,slew,0.)
                if tick==100:
                    target=yaw.clone()+torch.sign(base[:,2])*kick
                    previous_turn=torch.zeros_like(yaw,dtype=torch.bool)
                if tick>=100:
                    turning=cmd[:,2].abs()>1e-6
                    released=previous_turn & ~turning
                    target[released]=yaw[released]
                    target=target+slew*.02
                    error=torch.minimum(torch.maximum(torch.remainder(target-yaw+math.pi,2*math.pi)-math.pi,-cap),cap)
                    target=torch.remainder(yaw+error+math.pi,2*math.pi)-math.pi
                    cmd[:,2]=error
                    previous_turn=turning
                term.vel_command_b[:]=cmd;term.time_left.fill_(100.)
                rows=obs['actor'].cpu().numpy().copy();rows[:,48:51]=cmd.cpu().numpy();rows[:,51:61]=0.
                action=np.concatenate([net.run(None,{net.get_inputs()[0].name:row[None]})[0] for row in rows])
                assert action.shape==(n,14) and np.isfinite(action).all()
                with torch.no_grad():obs,_,done,timeout,_=env.step(torch.as_tensor(action,device=env.device))
                tilt=torch.acos((-robot.data.projected_gravity_b[:,2]).clamp(-1,1))
                failed=(done|timeout|(tilt>math.pi/4)).cpu().numpy()
                first_failure[failed & ~bad]=(tick+1)*.02;bad|=failed
                pos=(robot.data.root_link_pos_w.cpu().numpy()-origins)[:,:2]
                yaw=yaw_of(robot.data.root_link_quat_w).cpu().numpy()
                vel=robot.data.root_link_lin_vel_b.cpu().numpy()[:,:2]
                rate=robot.data.root_link_ang_vel_b[:,2].cpu().numpy()
                histories.append(np.column_stack([pos,yaw,vel,rate,tilt.cpu().numpy(),cmd[:,2].cpu().numpy(),cmd[:,0].cpu().numpy(),wire[:,2].cpu().numpy(),slew.cpu().numpy()]).copy())
                if seed==291601 and tick%2==0:qposes.append(env.sim.data.qpos[::16].cpu().numpy().copy())
            trace=np.asarray(histories)
            all_traces.append(trace)
            if seed==291601:
                np.savez_compressed(out/'render-poses.npz',qpos=np.asarray(qposes),origins=origins[::16])
            start=trace[99];end=trace[599];stop=trace[-1]
            for k,c in enumerate(cases):
                for rep in range(16):
                    i=k*16+rep;a=start[i,2];delta=end[i,:2]-start[i,:2];coast=stop[i,:2]-end[i,:2]
                    xy=trace[100:600,i,:2]-start[i,:2]
                    lateral=-xy[:,0]*math.sin(a)+xy[:,1]*math.cos(a)
                    dyaw=np.unwrap(trace[99:600,i,2])-a
                    all_rows.append(dict(case=c['id'],seed=seed,replicate=rep,survived=not bool(bad[i]),
                        failureSeconds=None if np.isnan(first_failure[i]) else float(first_failure[i]),
                        forwardMeters=float(delta[0]*math.cos(a)+delta[1]*math.sin(a)),
                        lateralMeters=float(-delta[0]*math.sin(a)+delta[1]*math.cos(a)),
                        maxCrossTrackMeters=float(np.abs(lateral).max()),maxExcursionMeters=float(np.linalg.norm(xy,axis=1).max()),pathMeters=float(np.linalg.norm(np.diff(trace[99:600,i,:2],axis=0),axis=1).sum()),releaseYawDegrees=float(np.rad2deg(np.unwrap(trace[599:,i,2])[-1]-trace[599,i,2])),yawChangeDegrees=float(np.rad2deg(dyaw[-1])),
                        meanForwardMps=float(trace[200:600,i,3].mean()),meanLateralMps=float(trace[200:600,i,4].mean()),
                        meanYawRate=float(trace[200:600,i,5].mean()),maxTiltDegrees=float(np.rad2deg(trace[:,i,6].max())),
                        releaseTravelMeters=float(np.linalg.norm(coast)),releaseFinalSpeedMps=float(np.linalg.norm(trace[-50:,i,3:5],axis=1).mean())))
            print(json.dumps({'seed':seed,'worlds':n,'survived':int((~bad).sum()),'seconds':16}),flush=True)
        finally:env.close()
    summary=[]
    for c in cases:
        rows=[r for r in all_rows if r['case']==c['id']];good=[r for r in rows if r['survived']]
        metrics=['maxExcursionMeters','pathMeters','releaseYawDegrees','forwardMeters','lateralMeters','maxCrossTrackMeters','yawChangeDegrees','meanForwardMps','meanYawRate','maxTiltDegrees','releaseTravelMeters','releaseFinalSpeedMps']
        summary.append(dict(**{k:v for k,v in c.items() if k!='schedule'},worlds=len(rows),survived=len(good),**{
            metric:dict(mean=float(np.mean([r[metric] for r in good])),min=float(min(r[metric] for r in good)),max=float(max(r[metric] for r in good))) if good else None for metric in metrics}))
    np.savez_compressed(out/'trajectories.npz',trace=np.stack(all_traces),seeds=[291601,291607],dt=.02)
    report=dict(policySha256=policy_sha,sourceHashesVerified=len(expected),source=str(source),hardwareAccess=False,
        nominalOnly=True,notExactObserverReplay=True,pairedInitialStates=False,limitations=['Ideal nominal sensor observations; no UART delay/encoder quantization/PD identification','No head input; no hardware-floor or wheel alignment identification','Training environment targets and physical bounds; not a hardware success rate'],
        phases=dict(settleSeconds=2,inputSeconds=10,releaseSeconds=4),cases=summary,worlds=all_rows)
    (out/'results.json').write_text(json.dumps(report,indent=2))
    print('DONE',flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--source',required=True);parser.add_argument('--output',required=True);parser.add_argument('--policy',required=True)
    run(parser.parse_args())
