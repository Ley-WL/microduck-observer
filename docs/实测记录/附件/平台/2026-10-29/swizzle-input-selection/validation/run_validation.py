import argparse,json,time,resource,hashlib
from pathlib import Path
from evaluate_sweep import run
p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--policy',required=True);p.add_argument('--output',required=True);a=p.parse_args()
assert hashlib.sha256(Path(a.policy).read_bytes()).hexdigest()=='539e6b2e7e4c7603ddd35ef27bc06742b292f9f3eede567f82111c0975375160'
cases=[]
for name,floor,top in [('linear075',0.,.75),('floor060',.6,.75),('floor065',.65,.8),('floor070',.7,.8)]:
 for throttle,strength,label in [(0.,.1,'small'),(0.,.35/.85,'half'),(0.,1.,'full'),(.1,1.,'push01'),(.2,1.,'push02'),(.4,1.,'forward'),(-.4,1.,'reverse')]:
  rate=floor+(top-floor)*strength
  for direction,side in [(1.,'left'),(-1.,'right')]:
   cases.append(dict(id=f'{name}_{label}_{side}',command=[throttle,0.,direction*.25*strength],mode='validation',gain=rate/(.25*strength),cap=.5,kick=0.,candidate=name,targetRate=rate))
t=time.monotonic();run(a,case_defs=cases)
Path(a.output,'runtime.json').write_text(json.dumps(dict(elapsedSeconds=time.monotonic()-t,peakRssKiB=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,training=False,hardwareAccess=False),indent=2))
