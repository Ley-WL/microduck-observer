"""Frozen source/model audit and independent replay; never accesses the UART."""
from pathlib import Path
import hashlib, json, subprocess, urllib.request, time
import numpy as np
import onnxruntime as ort

root=Path(__file__).resolve().parents[1]
evidence=root/'docs/实测记录/附件/平台/2026-10-11/kissqy-native'
evidence.mkdir(parents=True,exist_ok=True)
donor=root/'docs/实测记录/附件/平台/2026-10-11/getup-compare'
commit='94328f5b43c8ac825c90f0e564e7d1b926cd770d'
base=f'https://raw.githubusercontent.com/kissqy/Microduck-kissqy/{commit}/microduck-hd1910/firmware/policies/'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
models={}
def fetch(url,target):
    if target.is_file():return
    for attempt in range(3):
        try:
            data=urllib.request.urlopen(url,timeout=25).read()
            target.write_bytes(data);return
        except OSError:
            if attempt==2:raise
            time.sleep(1)
for local,slot in [('xgoduck_walk','walk'),('xgoduck_getup','stand'),('alpha_sitstand','sitstand'),('xgoduck_pick','ground_pick'),('xgoduck_roulade','roulade')]:
    path=evidence/('donor-'+slot+'.onnx')
    fetch(base+slot+'/policy.onnx',path)
    metadata=json.loads((root/'models'/f'{local}.metadata.json').read_text(encoding='utf-8'))
    actual=sha(root/'models'/f'{local}.onnx')
    assert actual==sha(path)==metadata['policySha256'], (local,actual,sha(path))
    models[local]={'sha256':actual,'upstreamPath':base+slot+'/policy.onnx','matched':True}
    for file in ['manifest.json','deployment-contract.json','deployment-runtime.json']:
        target=evidence/('donor-'+slot+'-'+file)
        fetch(base+slot+'/'+file,target)

# New rollout fixture is independent ORT output against the existing physical
# observation fixture, with identical HOME/order; no sensor/zero recalibration.
fixture=json.loads((root/'backend-rust/tests/fixtures/policy-xgoduck_getup.json').read_text(encoding='utf-8'))
fixture['kind']='xgoduck_roulade'
session=ort.InferenceSession(str(root/'models/xgoduck_roulade.onnx'),providers=['CPUExecutionProvider'])
fixture['action']=session.run(None,{session.get_inputs()[0].name:np.array([fixture['observation']],dtype=np.float32)})[0][0].tolist()
(root/'backend-rust/tests/fixtures/policy-xgoduck_roulade.json').write_text(json.dumps(fixture),encoding='utf-8')

trace=json.loads((root/'docs/实测记录/附件/平台/2026-10-11/getup-after-mechanical/getup-1791685669674.json').read_text(encoding='utf-8'))
session=ort.InferenceSession(str(root/'models/xgoduck_getup.onnx'),providers=['CPUExecutionProvider'])
errors=[]
for frame in trace['frames']:
    if frame.get('type')!='inference':continue
    action=session.run(None,{session.get_inputs()[0].name:np.array([frame['observation']],dtype=np.float32)})[0][0]
    errors.append(float(np.max(np.abs(action-np.array(frame['rawActionRadians'])))))
assert len(errors)==378 and max(errors)<1e-5
audit={'sourceCommit':commit,'models':models,'getupReplay':{'frames':len(errors),'maxRawActionErrorRadians':max(errors),'closedLoop':False},
       'calibrationChanged':False,'trainingPerformed':False,'hardwareMotion':False,
       'runtimeReference':'pinned source defaults plus per-policy export contract; donor live settings unavailable',
       'retainedDeviceAdapter':['local encoder references/directions','calibrated external IMU transport','EEPROM boundaries','fault/voltage/IMU freshness protection','user persistent model gains'],
       'notProven':['donor live servo-test overrides and torque cap','physical dynamics/contact parity','successful real recovery']}
options=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2','-o','HostKeyAlias=192.168.31.193']
reply=subprocess.run(['ssh',*options,'-p','5722','radxa@8.133.210.70','curl -fsS --max-time 4 http://127.0.0.1:8877/api/v1/snapshot'],capture_output=True,check=True)
snapshot=json.loads(reply.stdout)
(evidence/'snapshot-before.json').write_text(json.dumps(snapshot),encoding='utf-8')
rows=snapshot['joints']['data']['servos'];control=snapshot['system']['data']['servoControl']
audit['boardBefore']={'state':control['state'],'allOnline':all(r['online'] for r in rows),'allDisabled':all(r['torque']==0 for r in rows)}
audit['evidenceHashes']={f.name:sha(f) for f in evidence.iterdir() if f.is_file() and f.name!='audit.json'}
(evidence/'audit.json').write_text(json.dumps(audit,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'modelsMatched':len(models),'getupReplay':audit['getupReplay'],'boardBefore':audit['boardBefore']}))
