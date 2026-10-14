"""Offline fixtures for the exact Rust roll exit guard; no device connection."""
from pathlib import Path
import hashlib,json,numpy as np
root=Path(__file__).resolve().parents[1]
out=root/'docs/实测记录/附件/平台/2026-10-12/roll-exit'
out.mkdir(parents=True,exist_ok=True)
rows=[];sources={}
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
screen=root.parent/'microduck-rl/artifacts/roulade-transfer-20261012/complete/evidence/roulade-transfer/baseline-screen-v3'
for p in sorted(screen.glob('*.npz')):
    sources[str(p.relative_to(root.parent))]=sha(p)
    f=np.load(p);q=f['qpos'][:,:,3:7];w,x,y,z=np.moveaxis(q,-1,0)
    gravity=np.stack([2*(w*y-x*z),-2*(y*z+w*x),2*(x*x+y*y)-1],axis=-1)
    for world in range(q.shape[1]):
        name=p.stem+'-'+str(world)
        for t in range(q.shape[0]):
            rows.append((name,(t+1)*.02,*f['gyro'][t,world],*gravity[t,world]))
for filename in ['new-policy-session-latest.json','fixed-policy-session-latest.json']:
    p=root/'docs/实测记录/附件/平台/2026-10-12/roulade-diagnosis'/filename
    sources[str(p.relative_to(root))]=sha(p)
    frames=[f for f in json.loads(p.read_text(encoding='utf-8'))['frames'] if f.get('kind')=='xgoduck_roulade' and 'observation' in f]
    for i,f in enumerate(frames):rows.append((filename,(i+1)*.02,*f['observation'][:6]))
(out/'fixtures.tsv').write_text('\n'.join('\t'.join(map(str,r)) for r in rows)+'\n',encoding='utf-8')
harness='''#[path="../../../../../../../../backend-rust/src/roll_exit.rs"] mod roll_exit;
use std::io::{self,BufRead};
fn main(){let mut guard=roll_exit::RollExit::default();let mut name=String::new();let mut ended=false;let mut t=0.;
for line in io::stdin().lock().lines(){let line=line.unwrap();let p:Vec<_>=line.split('\\t').collect();if name!=p[0]{if !name.is_empty() && !ended{println!("{}\\tincomplete-record\\t{}\\t{}",name,t,guard.rotation_degrees);}guard.reset();ended=false;name=p[0].into();}if ended{continue;}
t=p[1].parse().unwrap();let obs:Vec<f32>=p[2..].iter().map(|v|v.parse().unwrap()).collect();guard.sample(&obs);
if guard.complete(t){println!("{}\\tcomplete\\t{}\\t{}",name,t,guard.rotation_degrees);ended=true;}
else if t>=roll_exit::RollExit::TIMEOUT-1e-9{println!("{}\\ttimeout\\t{}\\t{}",name,t,guard.rotation_degrees);ended=true;}
}if !name.is_empty() && !ended{println!("{}\\tincomplete-record\\t{}\\t{}",name,t,guard.rotation_degrees);}}'''
# Absolute WSL path avoids depending on a fragile depth count.
module='/mnt/'+str(root/'backend-rust/src/roll_exit.rs').replace('\\','/').replace('D:','d')
harness=harness.replace('../../../../../../../../backend-rust/src/roll_exit.rs',module)
(out/'replay.rs').write_text(harness,encoding='utf-8')
(out/'sources.json').write_text(json.dumps(sources,indent=2),encoding='utf-8')
print(out)
