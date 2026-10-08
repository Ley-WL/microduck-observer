"""Read-only policy shadow on a user-supported static stance; never start the policy."""
import json,time,urllib.request,urllib.error,math
import sys
from pathlib import Path

base='http://192.168.31.186:8877/api/v1/'
root=Path(__file__).resolve().parents[1]
directory=root/'docs/实测记录/附件/HAT/2026-10-04/model-home-supported'
def get():return json.load(urllib.request.urlopen(base+'snapshot',timeout=5))
before=get();report={'before':before,'shadowRuns':[],'samples':[]}
end=time.monotonic()+5
while time.monotonic()<end:
    report['samples'].append(get());time.sleep(.1)

assert before['system']['data']['servoControl']['state'] in ('holding','shadow')
for _ in range(3):
    request=urllib.request.Request(base+'policy/shadow',data=json.dumps({'revision':before['calibration']['revision']}).encode(),headers={'Content-Type':'application/json'},method='POST')
    try:
        response=urllib.request.urlopen(request,timeout=10)
        result=json.load(response)
        snap=get(); rows={r['id']:r for r in snap['joints']['data']['servos']}
        deltas={id:(goal-rows[int(id)]['position'])*360/4096*before['calibration']['joints']['directions'].get(id,-1) for id,goal in result.get('targets',{}).items()}
        report['shadowRuns'].append({'result':result,'targetDeltaDegrees':deltas,'snapshot':snap})
    except urllib.error.HTTPError as error:
        report['error']={'status':error.code,'detail':json.load(error)};break
    time.sleep(.2)
report['after']=get()
fields=['torque','goalPositionRaw','goalCurrentRaw','accelerationRaw','speedLimitRaw','torqueLimitRaw','kpRaw','kiRaw','kdRaw']
def registers(snap):return {r['id']:{key:r[key] for key in fields} for r in snap['joints']['data']['servos'] if r['online']}
report['registersUnchanged']=registers(before)==registers(report['after'])
report['calibrationUnchanged']=before['calibration']==report['after']['calibration']
directory.mkdir(parents=True,exist_ok=True)
(directory/('shadow-after-reference.json' if '--after-reference' in sys.argv else 'shadow.json')).write_text(json.dumps(report),encoding='utf-8')
print(json.dumps({'registersUnchanged':report['registersUnchanged'],'error':report.get('error'),
                  'runs':[{'commandCount':r['result'].get('commandCount'),'inferenceMs':r['result'].get('inferenceMs'),'deltaDegrees':r['targetDeltaDegrees']} for r in report['shadowRuns']]}))
