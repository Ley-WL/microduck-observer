"""Frozen CPU single-factor probe: current mechanical output clipping only."""
import hashlib,json,os,subprocess
from pathlib import Path
import numpy as np
import onnxruntime as ort

root=Path(__file__).resolve().parents[2]
out=root/'evidence/getup-mechanical-20261012'
assert not out.exists(),'Probe already exists; inspect results instead of repeating'
out.mkdir()
source=root/'scripts/evaluate_skills_cpu.py'
text=source.read_text()
needle='                        target = home+filtered\n'
assert text.count(needle)==1
ranges=[[-25,30],[-22,22],[-90,90],[-90,90],[-70,70],[-90,70],[-90,90],[-170,170],[-20,20],[-30,25],[-22,22],[-90,90],[-90,90],[-70,70]]
text=text.replace(needle,needle+f"                        if kind == 'getup':\n                            mechanical = np.radians(np.asarray({ranges!r}))\n                            target = np.clip(target, mechanical[:,0], mechanical[:,1])\n")
text=text.replace("'accepted': len(groups)","'diagnosticMechanicalClip': True, 'accepted': False and len(groups)")
probe=root/'scripts/evaluate_getup_mechanical_preview_20261012.py'
assert not probe.exists()
probe.write_text(text)
session=ort.InferenceSession(str(root/'policies/getup.onnx'),providers=['CPUExecutionProvider'])
rows=json.loads(Path(__file__).with_name('ort-inputs.json').read_text())
errors=[float(np.max(np.abs(session.run(None,{session.get_inputs()[0].name:np.asarray(r['observation'],dtype=np.float32)[None]})[0][0]-np.asarray(r['action'])))) for r in rows]
(out/'real-ort-parity.json').write_text(json.dumps({'frames':len(rows),'maxAbsErrorRad':max(errors),'hardwareAccess':False}))
manifest={'diagnosticOnly':True,'training':False,'mechanicalClipDegrees':ranges,'seed':15601,'trials':2,
          'boundary':'Only output target clipping changes; XML physical stops/contact/BAM remain original. Not whole hardware reproduction.',
          'sourceHashes':{str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [source,probe,root/'scripts/bam_cpu_controller.py',root/'policies/getup.onnx',root/'policies/getup.metadata.json']}}
(out/'manifest.json').write_text(json.dumps(manifest,indent=2))
env=dict(os.environ,OMP_NUM_THREADS='1',OPENBLAS_NUM_THREADS='1',MUJOCO_GL='egl',LIBGL_ALWAYS_SOFTWARE='1')
with (out/'job.log').open('w') as log:
    r=subprocess.run([str(root.parent/'microduck-rl/.venv/bin/python'),str(probe),'--onnx',str(root/'policies/getup.onnx'),'--output',str(out),'--seeds','15601','--trials','2','--capture-policy-frames','--video'],cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
(out/'job.exit').write_text(str(r.returncode))
print((out/'job.log').read_text())
assert r.returncode==0
