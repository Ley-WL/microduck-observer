import argparse,json,time,resource,hashlib
from pathlib import Path
import numpy as np
from evaluate_sweep import run
p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--policy',required=True);p.add_argument('--output',required=True);a=p.parse_args()
assert hashlib.sha256(Path(a.policy).read_bytes()).hexdigest()=='539e6b2e7e4c7603ddd35ef27bc06742b292f9f3eede567f82111c0975375160'
cases=[]
for name,v,rate in [('baseline',0.,.5),('selected',0.,.9),('forward',.4,.5),('reverse',-.4,.5)]:
 for direction,side in [(1.,'left'),(-1.,'right')]:
  cases.append(dict(id=name+'_'+side,command=[v,0.,direction*.25],mode='transient',gain=rate*4,cap=.5,kick=0.))
t=time.monotonic();run(a,case_defs=cases)
x=np.load(Path(a.output,'trajectories.npz'))['trace'];report=[]
for k,c in enumerate(cases):
 for start,end,sign in [(100,250,1),(350,500,-1),(600,750,1)]:
  vals=[];exc=[];release=[]
  for seed in range(2):
   for rep in range(32):
    z=x[seed,:,k*32+rep];yaw=np.unwrap(z[start-1:end,2]);vals.append(float(np.rad2deg(yaw[-1]-yaw[0])*np.sign(c['command'][2])*sign))
    exc.append(float(np.linalg.norm(z[start:end,:2]-z[start-1,:2],axis=1).max()))
    y=np.unwrap(z[end-1:min(end+100,800),2]);release.append(float(np.rad2deg(y[-1]-y[0])))
  report.append(dict(case=c['id'],startSeconds=start*.02,durationSeconds=(end-start)*.02,worlds=len(vals),directedYawDegrees=dict(mean=float(np.mean(vals)),min=min(vals),max=max(vals)),less10=sum(v<10 for v in vals),wrongDirection=sum(v<0 for v in vals),maxExcursionMeters=max(exc),releaseYawDegrees=dict(min=min(release),max=max(release))))
Path(a.output,'transient.json').write_text(json.dumps(dict(phases='2s settle, 3s first turn, 2s release, 3s opposite, 2s release, 3s first, 1s release',cases=report,elapsedSeconds=time.monotonic()-t,peakRssKiB=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss),indent=2))
