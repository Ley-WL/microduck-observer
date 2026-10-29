import argparse,json,time,resource,hashlib
from pathlib import Path
from evaluate_sweep import run
p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--policy',required=True);p.add_argument('--output',required=True);a=p.parse_args()
assert hashlib.sha256(Path(a.policy).read_bytes()).hexdigest()=='539e6b2e7e4c7603ddd35ef27bc06742b292f9f3eede567f82111c0975375160'
cases=[]
for name,throttle,rate in [('still080',0.,.8),('still085',0.,.85),('still090',0.,.9),('forward030',.3,.5),('forward040',.4,.5),('reverse030',-.3,.5),('reverse040',-.4,.5)]:
 for direction,side in [(1.,'left'),(-1.,'right')]:
  cases.append(dict(id=name+'_'+side,command=[throttle,0.,direction*.25],mode='heldout',gain=rate*4,cap=.5,kick=0.,targetRate=rate))
t=time.monotonic();run(a,case_defs=cases)
Path(a.output,'runtime.json').write_text(json.dumps(dict(elapsedSeconds=time.monotonic()-t,peakRssKiB=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,training=False,hardwareAccess=False),indent=2))
