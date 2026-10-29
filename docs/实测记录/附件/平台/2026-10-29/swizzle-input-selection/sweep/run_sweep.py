import argparse,json,time,resource,hashlib
from pathlib import Path
from evaluate_sweep import run
p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--policy',required=True);p.add_argument('--output',required=True);a=p.parse_args()
assert hashlib.sha256(Path(a.policy).read_bytes()).hexdigest()=='539e6b2e7e4c7603ddd35ef27bc06742b292f9f3eede567f82111c0975375160'
cases=[]
configs=[(f'rate{r}',0.,r*4.,.5,0.) for r in [.2,.3,.5,.75,1.]]
configs += [(f'cap{c}',0.,2.,c,0.) for c in [.2,.3,.4]]
configs += [(f'kick{k}',0.,2.,.5,k) for k in [.2,.35,.5]]
configs += [(f'push{v}',v,2.,.5,0.) for v in [-.15,-.1,-.05,.025,.05,.075,.1,.125,.15,.2,.25]]
for name,v,gain,cap,kick in configs:
 for direction,label in [(1.,'left'),(-1.,'right')]:
  cases.append(dict(id=name+'_'+label,command=[v,0.,direction*.25],mode='sweep',gain=gain,cap=cap,kick=kick))
t=time.monotonic();run(a,case_defs=cases)
Path(a.output,'runtime.json').write_text(json.dumps(dict(elapsedSeconds=time.monotonic()-t,peakRssKiB=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,training=False,hardwareAccess=False),indent=2))
