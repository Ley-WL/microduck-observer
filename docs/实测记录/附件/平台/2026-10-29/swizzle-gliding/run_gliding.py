import argparse,json,time,resource,hashlib
from pathlib import Path
import numpy as np
from evaluate_gliding import run
p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--policy',required=True);p.add_argument('--output',required=True);a=p.parse_args()
assert hashlib.sha256(Path(a.policy).read_bytes()).hexdigest()=='539e6b2e7e4c7603ddd35ef27bc06742b292f9f3eede567f82111c0975375160'
cases=[]
for v in [-.4,-.2,-.1,-.02,.02,.1,.2,.4]:
 for turn in [.025,.125,.25]:
  for sign in [1.,-1.]:
   cmd=[v,0.,sign*turn];schedule=np.zeros((800,3));schedule[100:600]=cmd
   cases.append(dict(id=f'grid_v{v:+.2f}_t{sign*turn:+.3f}',command=cmd,mode='grid',schedule=schedule.tolist(),segments=[(100,600,sign,'hold')]))
for v in [-.4,-.1,.1,.4]:
 for sign in [1.,-1.]:
  cmd=[v,0.,sign*.25];schedule=np.zeros((800,3));schedule[100:250]=cmd
  schedule[250:350]=[v,0.,0.];schedule[350:500]=[v,0.,-sign*.25]
  schedule[500:600]=[0.,0.,-sign*.25]
  for tick in range(600,750):schedule[tick]=[np.sign(v)*(.004 if (tick//5)%2 else 0.),0.,-sign*.25]
  cases.append(dict(id=f'switch_v{v:+.2f}_s{sign:+.0f}',command=cmd,mode='switch',schedule=schedule.tolist(),segments=[(100,250,sign,'glide'),(250,350,0.,'straight'),(350,500,-sign,'reverse_turn'),(500,600,-sign,'centered'),(600,750,-sign,'deadzone_crossing'),(750,800,0.,'release')]))
t=time.monotonic();run(a,case_defs=cases)
x=np.load(Path(a.output,'trajectories.npz'))['trace'];results=json.loads(Path(a.output,'results.json').read_text());report=[];worlds=[]
for k,c in enumerate(cases):
 for start,end,sign,label in c['segments']:
  vals=[];tilts=[];speeds=[];fail=0
  for seed in range(2):
   for rep in range(16):
    z=x[seed,:,k*16+rep];yaw=np.unwrap(z[start-1:end,2]);angle=float(np.rad2deg(yaw[-1]-yaw[0]));directed=angle*sign if sign else angle
    vals.append(directed);tilts.append(float(np.rad2deg(z[start:end,6].max())));speeds.append(float(z[start:end,3].mean()))
    row=results['worlds'][seed*len(cases)*16+k*16+rep];fail+=not row['survived']
    worlds.append(dict(case=c['id'],phase=label,seed=seed,replicate=rep,yawDegrees=angle,directedDegrees=directed,survived=row['survived']))
  expected=abs(c['command'][2]*2)*(end-start)*.02*180/np.pi if c['mode']=='grid' else None
  report.append(dict(case=c['id'],phase=label,seconds=(end-start)*.02,worlds=len(vals),failed=fail,directedYawDegrees=dict(mean=float(np.mean(vals)),min=min(vals),max=max(vals)),wrongDirection=sum(v<0 for v in vals) if sign else None,less10=sum(v<10 for v in vals) if sign else None,referenceTurnDegrees=expected,lessHalfReference=sum(v<expected*.5 for v in vals) if expected else None,maxTiltDegrees=max(tilts),meanForwardMps=float(np.mean(speeds))))
Path(a.output,'gliding.json').write_text(json.dumps(dict(cases=report,worlds=worlds,elapsedSeconds=time.monotonic()-t,peakRssKiB=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,traceColumns=['x','y','yaw','vx','vy','yawRate','tilt','actorHeadingError','wireThrottle','wireTurn','targetSlewRate']),indent=2))
