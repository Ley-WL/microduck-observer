"""Explicit user-authorized HOME-only action. No policy inference."""
import importlib.util, json, subprocess, sys, time, urllib.request
from pathlib import Path
sys.path.insert(0, '/home/radxa/microduck-observer/releases/20261002-servo-scan-recovery/debug-server')
from servos import ReadOnlyBus
spec = importlib.util.spec_from_file_location('home_control', '/home/radxa/servo_control_home_hold.py')
control = importlib.util.module_from_spec(spec)
spec.loader.exec_module(control)
with urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot', timeout=5) as r:
    before = json.load(r)
assert before['system']['data']['servoControl']['state'] not in ['policy', 'preflight', 'moving', 'disabling']
cal = json.loads(Path('/var/lib/microduck-observer/calibration.json').read_text())
meta = json.loads(Path('/home/radxa/microduck-observer/current/models/hd1910-head-v5.metadata.json').read_text())
ids = [20,21,22,23,24,30,31,32,33,10,11,12,13,14]
targets = dict(zip(ids, meta['homeRadians']))
out = {'mode':'v5-home-only-no-policy', 'started': time.time(), 'before':before['joints'], 'calibration':cal, 'homeRadians':targets, 'states':[]}
bus = None
stopped = False
try:
    subprocess.run(['systemctl','stop','microduck-observer'], check=True)
    stopped = True
    bus = ReadOnlyBus('/dev/ttyS2')
    out['result'] = control.execute_control(bus, {'action':'stand','calibration':cal,'standTargets':targets}, lambda:False, lambda rows:None, out['states'].append)
except Exception as e:
    out['error'] = str(e)
finally:
    if bus:
        out['serialTrace'] = list(bus.trace)
        out['lastRead'] = bus.last_read
        try:
            out['after'] = bus.read_feedback_many(control.ALL_IDS)
        except Exception as e:
            out['afterError'] = str(e)
        bus.close()
    if stopped:
        subprocess.run(['systemctl','start','microduck-observer'], check=True)
    Path('/home/radxa/v5-home-hold-once-20261003.json').write_text(json.dumps(out,ensure_ascii=False))
print(json.dumps({k:out[k] for k in ['result','error','afterError'] if k in out},ensure_ascii=False))
if 'after' in out:
    print(json.dumps({i:{k:r[k] for k in ['torque','position','target','fault','voltage']} for i,r in out['after'].items()}))
