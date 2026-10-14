from pathlib import Path
import json,hashlib,numpy as np,onnxruntime as ort
root=Path(__file__).resolve().parent
expected=json.loads((root/'expected-source-hashes.json').read_text())
assert all(hashlib.sha256((root/n).read_bytes()).hexdigest()==h for n,h in expected['sourceHashes'].items())
opt=ort.SessionOptions();opt.intra_op_num_threads=opt.inter_op_num_threads=1
s=ort.InferenceSession(str(root/'roulade.onnx'),sess_options=opt,providers=['CPUExecutionProvider'])
results=[]
for name in ['before.json','after.json']:
 d=json.loads((root/name).read_text());frames=[f for f in d['frames'] if f.get('kind')=='xgoduck_roulade' and f.get('type')=='inference']
 error=max(float(np.max(np.abs(s.run(None,{s.get_inputs()[0].name:np.array(f['observation'],dtype=np.float32)[None]})[0][0]-np.array(f['rawActionRadians'])))) for f in frames)
 results.append({'file':name,'frames':len(frames),'maxRawActionErrorRadians':error})
report={'sourceHashesExactlyPreserved':True,'realInputInferenceChecks':results,'training':False,'hardwareCommandsAdded':False,'gpuAccess':False,'fullRollAcceptance':False}
(root/'final-audit.json').write_text(json.dumps(report,indent=2));print(json.dumps(report))
