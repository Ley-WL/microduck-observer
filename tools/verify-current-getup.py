"""Board-only read-only check of the already-installed validated getup actor."""
import datetime,hashlib,json,os,subprocess,time,urllib.request
from pathlib import Path

root=Path('/home/radxa/microduck-observer/current').resolve()
expected='ec665f3121d8d45da0abd3d5792955d0aa46ba39f2897e3bbe01788fda9770f4'
policy=root/'models/xgoduck_getup.onnx'
assert hashlib.sha256(policy.read_bytes()).hexdigest()==expected
get=lambda endpoint:json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/'+endpoint,timeout=5))
info=get('policy');entry=next(m for m in info['models'] if m['kind']=='xgoduck_getup')
assert entry['available'] and entry['metadata']['policySha256']==expected
environment=os.environ.copy()
environment['ORT_DYLIB_PATH']=str(root/'lib/libonnxruntime.so')
environment['MICRODUCK_POLICY_PATH']=str(root/'models/hd1910-head-v5.onnx')
fixture=Path(__file__).resolve().parent/'policy-xgoduck_getup.json'
checked=subprocess.run([str(root/'bin/microduck-observer'),'--verify-policy-fixture',str(fixture)],
    env=environment,check=True,capture_output=True,text=True)
parity=json.loads(checked.stdout)
samples=[]
for _ in range(20):
    snapshot=get('snapshot')
    rows=snapshot['joints']['data']['servos']
    samples.append({'seq':snapshot['joints']['seq'],'onlineIds':[r['id'] for r in rows if r['online']],
        'enabledIds':[r['id'] for r in rows if r['torque']],
        'missingIds':snapshot['joints']['data']['diagnostics']['missingIds'],
        'control':snapshot['system']['data']['servoControl']})
    time.sleep(.05)
result={'checkedAt':datetime.datetime.now(datetime.timezone(datetime.timedelta(hours=8))).isoformat(),
    'release':str(root),'policySha256':expected,'available':True,'armParity':parity,
    'samples':samples,'hardwareMotionAdded':False,'serviceRestarted':False,'modelReplaced':False,
    'reason':'Already installed actor exactly matches the actor used in current getup simulation probes.',
    'hardwareValidated':False}
Path(__file__).with_name('verification.json').write_text(json.dumps(result,indent=2))
print(json.dumps({'release':str(root),'policySha256':expected,'available':True,'armParity':parity,
    'snapshots':len(samples),'all15Online':all(len(s['onlineIds'])==15 for s in samples),
    'anyMissing':any(s['missingIds'] for s in samples),'enabledIds':samples[-1]['enabledIds'],
    'controlState':samples[-1]['control']['state'],'hardwareMotionAdded':False}))
