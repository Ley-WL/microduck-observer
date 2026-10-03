"""Authorized +/-11 encoder step HTTP verification on #23 only."""
import json, time, urllib.request
from pathlib import Path
base = 'http://192.168.31.193:8877'
def get(route):
    with urllib.request.urlopen(base+route, timeout=8) as r:
        return json.load(r)
def row():
    return next(r for r in get('/api/v1/health')['joints']['data']['servos'] if r['id'] == 23)
cal = get('/api/v1/calibration')
before = row()
assert before['online'] and before['torque'] == 1 and before['fault'] == 0
start = before['position']
ref = cal['joints']['references']['23']
direction = cal['joints']['directions'].get('23', -1)
limits = get('/api/v1/servos/limits')['limits']['23']
output = {'before': before, 'phases': []}
for goal in [start-11, start]:
    angle = (goal-ref)*360/4096/direction
    assert limits[0] <= angle <= limits[1]
    req = urllib.request.Request(base+'/api/v1/servos/angle', json.dumps({'id':23,'angleDeg':angle,'revision':cal['revision']}).encode(), {'Content-Type':'application/json'}, method='POST')
    with urllib.request.urlopen(req, timeout=10) as r:
        result = json.load(r)
    samples = []
    for _ in range(12):
        samples.append(row())
        time.sleep(.05)
    output['phases'].append({'goal':goal,'response':result,'samples':samples})
    print('goal',goal,'internal',samples[-1]['target'],'position',samples[-1]['position'])
path = Path(__file__).resolve().parents[1]/'docs/实测记录/附件/平台/2026-10-02/servo23-http-fix-verification.json'
path.write_text(json.dumps(output, ensure_ascii=False), encoding='utf-8')
assert all(p['samples'][-1]['goalPositionRaw'] == p['goal'] and p['samples'][-1]['target'] == p['goal'] for p in output['phases'])
assert abs(output['phases'][0]['samples'][-1]['position']-start) >= 5
assert abs(output['phases'][1]['samples'][-1]['position']-output['phases'][0]['samples'][-1]['position']) >= 5
print('Actual motion and internal target update verified both ways')
