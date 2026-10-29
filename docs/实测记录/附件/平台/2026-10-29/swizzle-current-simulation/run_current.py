"""Evaluate frozen deployed policy with current physical gamepad mapping; no training."""
import argparse, hashlib, json, platform, resource, time
from pathlib import Path
import onnxruntime, torch, mujoco
from evaluate_heading import run

def axis(value):
    return 0. if abs(value)<=.15 else (1. if value>0 else -1.)*(abs(value)-.15)/.85

def command(x,y):
    return [-axis(y)*.4,0.,-axis(x)*.25]

p=argparse.ArgumentParser();p.add_argument('--source',required=True);p.add_argument('--policy',required=True);p.add_argument('--output',required=True);a=p.parse_args()
assert hashlib.sha256(Path(a.policy).read_bytes()).hexdigest()=='539e6b2e7e4c7603ddd35ef27bc06742b292f9f3eede567f82111c0975375160'
cases=[dict(id=name,stick=[x,y],command=command(x,y),mode='physical') for name,x,y in [
 ('neutral',0.,0.),('straight',0.,-1.),('stationary_left',-1.,0.),('stationary_right',1.,0.),
 ('half_left',-.5,0.),('half_right',.5,0.),('forward_left',-1.,-1.),('forward_right',1.,-1.),
 ('reverse_left',-1.,1.),('reverse_right',1.,1.)]]
assert command(-1.,-1.)==[.4,0.,.25]
assert command(1.,0.)==[0.,0.,-.25]
start=time.monotonic();run(a,case_defs=cases)
Path(a.output,'runtime.json').write_text(json.dumps(dict(elapsedSeconds=time.monotonic()-start,peakProcessRssKiB=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,python=platform.python_version(),torch=torch.__version__,onnxruntime=onnxruntime.__version__,mujoco=mujoco.__version__,device=torch.cuda.get_device_name(),physicalGpu=5,rawAxisDeadzone=.15,hardwareAccess=False,training=False),indent=2))
