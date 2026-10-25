"""Native inference-only phase parity; never contacts the robot or UART."""
import json,os,subprocess,tempfile
from pathlib import Path
import onnxruntime
root=Path(__file__).resolve().parents[3]
out=Path(__file__).resolve().parent
library=next((Path(onnxruntime.__file__).parent/'capi').glob('libonnxruntime.so*'))
with tempfile.TemporaryDirectory(prefix='observer-crouch-verify-') as temporary:
 env=dict(os.environ,ORT_DYLIB_PATH=str(library),MICRODUCK_POLICY_PATH=str(root/'models/xgoduck_walk.onnx'),MICRODUCK_MODEL_REGISTRY_FILE=str(Path(temporary)/'model-registry.json'),MICRODUCK_GAINS_FILE=str(Path(temporary)/'model-gains.json'))
 binary=root/'backend-rust/target/release/microduck-observer'
 def run(arguments):
  result=subprocess.run([str(binary),*arguments],cwd=root/'backend-rust',env=env,capture_output=True,text=True)
  if result.returncode:raise RuntimeError(result.stdout+'\n'+result.stderr)
  return json.loads(result.stdout.splitlines()[-1])
 bank=run(['--verify-model-bank']);assert any(m['kind']=='swizzle_crouch' for m in bank['models'])
 (out/'native-model-bank.json').write_text(json.dumps(bank,indent=2),encoding='utf-8')
 reports=[]
 for tick in range(6):
  report=run(['--verify-policy-fixture',str(root/f'backend-rust/tests/fixtures/policy-roller-crouch-{tick}.json')])
  assert report['ok'] and not report['hardwareAccess'];reports.append(report)
 (out/'native-phase-parity.json').write_text(json.dumps(reports,indent=2),encoding='utf-8')
 print(json.dumps({'phasesPassed':len(reports),'models':len(bank['models']),'maxObservationError':max(r['observationMaxAbsError'] for r in reports),'maxActionError':max(r['actionMaxAbsError'] for r in reports),'hardwareAccess':False}))
