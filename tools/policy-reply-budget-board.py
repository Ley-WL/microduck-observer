import hashlib,json,subprocess,time,urllib.request
from pathlib import Path
base='http://127.0.0.1:8877/api/v1/'
def snapshot():return json.load(urllib.request.urlopen(base+'snapshot',timeout=3))
cal=Path('/var/lib/microduck-observer/calibration.json')
before=snapshot();rows=before['joints']['data']['servos']
assert before['system']['data']['servoControl']['state'] not in ('policy','preflight','moving','disabling')
assert len(rows)==15 and all(r['online'] and r['torque']==0 for r in rows)
manifest={'before':before,'calibrationBefore':json.loads(cal.read_text())}
subprocess.run(['systemctl','stop','microduck-observer'],check=True)
try:
 result=subprocess.run(['/home/radxa/policy-reply-budget-check','/home/radxa/policy-reply-budget.json'],timeout=25)
 manifest['returncode']=result.returncode
finally:
 subprocess.run(['systemctl','start','microduck-observer'],check=True)
 for _ in range(100):
  try:
   after=snapshot();rs=after['joints']['data']['servos']
   if len(rs)==15 and all(r['online'] and r.get('profileFresh') for r in rs):break
  except OSError:pass
  time.sleep(.1)
 manifest['after']=after;manifest['calibrationAfter']=json.loads(cal.read_text())
 Path('/home/radxa/policy-reply-budget-manifest.json').write_text(json.dumps(manifest))
assert manifest['returncode']==0
print(json.dumps({'returncode':manifest['returncode'],'allDisabled':all(r['torque']==0 for r in after['joints']['data']['servos'])}))
