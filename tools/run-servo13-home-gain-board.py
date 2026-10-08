"""Temporarily give the native acceptance binary sole ownership of UART2; always restore service."""
import hashlib,json,subprocess,time,urllib.request
import sys,os
from pathlib import Path

def snapshot():return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=4))
prefix='servo13-home-gain'
cal=Path('/var/lib/microduck-observer/calibration.json')
manifest={'before':snapshot(),'calibrationShaBefore':hashlib.sha256(cal.read_bytes()).hexdigest(),
          'kernelBefore':Path('/proc/tty/driver/serial').read_text()}
assert manifest['before']['system']['data']['servoControl']['state'] not in ('policy','moving','preflight','disabling')
assert len(manifest['before']['joints']['data']['servos'])==15
assert all(r['online'] and r['torque']==0 and r['fault']==0 for r in manifest['before']['joints']['data']['servos'])
subprocess.run(['systemctl','stop','microduck-observer'],check=True)
try:
    result=subprocess.run(['/home/radxa/servo13-gain-check','--supported-single13','/home/radxa/'+prefix+'-acceptance.json','--home'],timeout=50,env=dict(os.environ,MICRODUCK_POLICY_PATH='/home/radxa/microduck-observer/current/models/hd1910-head-v5.onnx'))
    manifest['returncode']=result.returncode
finally:
    # The binary unloads itself; if it was externally terminated, send one
    # explicit all-disable through the restarted platform below.
    subprocess.run(['systemctl','start','microduck-observer'],check=True)
    for _ in range(80):
        try:
            after=snapshot()
            if len(after['joints']['data']['servos'])==15 and all(r['online'] for r in after['joints']['data']['servos']):
                manifest['after']=after
                break
        except Exception:pass
        time.sleep(.1)
    if any(r.get('torque')!=0 for r in manifest.get('after',{}).get('joints',{}).get('data',{}).get('servos',[])):
        request=urllib.request.Request('http://127.0.0.1:8877/api/v1/servos/disable',data=b'{}',headers={'Content-Type':'application/json'},method='POST')
        manifest['emergencyDisable']=json.load(urllib.request.urlopen(request,timeout=8))
        time.sleep(.1);manifest['after']=snapshot()
    manifest['calibrationShaAfter']=hashlib.sha256(cal.read_bytes()).hexdigest()
    manifest['kernelAfter']=Path('/proc/tty/driver/serial').read_text()
    Path('/home/radxa/'+prefix+'-manifest.json').write_text(json.dumps(manifest))
print(json.dumps({'returncode':manifest.get('returncode'),'service':subprocess.check_output(['systemctl','is-active','microduck-observer'],text=True).strip(),
                  'calibrationUnchanged':manifest['calibrationShaBefore']==manifest['calibrationShaAfter'],
                  'allDisabled':all(r['online'] and r['torque']==0 for r in manifest['after']['joints']['data']['servos'])}),flush=True)
if manifest.get('returncode')!=0:raise SystemExit(1)
