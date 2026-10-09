"""Read current posture and fresh-start shadow targets; never move servos."""
import json,time,urllib.request,math,struct
from pathlib import Path
from datetime import datetime
root=Path(__file__).resolve().parents[1]
base='http://192.168.31.186:8877/api/v1/'
def get():return json.load(urllib.request.urlopen(base+'snapshot',timeout=5))
before=get()
assert before['system']['data']['servoControl']['state'] in ('idle','holding','shadow'), 'Active control: do not replace its context with shadow'
samples=[]
end=time.monotonic()+4
while time.monotonic()<end:
 samples.append(get());time.sleep(.1)
request=urllib.request.Request(base+'policy/shadow',data=json.dumps({'revision':before['calibration']['revision']}).encode(),headers={'Content-Type':'application/json'},method='POST')
result=json.load(urllib.request.urlopen(request,timeout=15));after=get()
ids=[20,21,22,23,24,30,31,32,33,10,11,12,13,14]
meta=json.loads((root/'models/hd1910-head-v5.metadata.json').read_text(encoding='utf-8'))
home=[struct.unpack('f',struct.pack('f',x))[0]*180/math.pi for x in meta['homeRadians']]
cal=after['calibration']['joints'];joints={row['id']:row for row in after['joints']['data']['servos']}
angles={id:(row['position']-cal['references'][str(id)])*360/4096*cal['directions'].get(str(id),-1) for id,row in joints.items()}
comparison=[]
for n,id in enumerate(ids):
 filtered=result['targetDegrees'][n]
 raw=home[n]+(filtered-home[n])/.55
 goal=result['targets'][str(id)]
 sent=(goal-cal['references'][str(id)])*360/4096*cal['directions'].get(str(id),-1)
 positions=[next(row['position'] for row in s['joints']['data']['servos'] if row['id']==id) for s in samples]
 comparison.append(dict(id=id,name=meta['jointOrder'][n],currentDegrees=angles[id],rawNetworkTargetDegrees=raw,rawChangeDegrees=raw-angles[id],filteredTargetDegrees=filtered,encodedTargetDegrees=sent,filteredChangeDegrees=sent-angles[id],observedPositionSpanDegrees=(max(positions)-min(positions))*360/4096,kpRaw=joints[id]['kpRaw'],kdRaw=joints[id]['kdRaw'],goalCurrentRaw=joints[id]['goalCurrentRaw'],speedLimitRaw=joints[id]['speedLimitRaw']))
fields=['torque','goalPositionRaw','goalCurrentRaw','speedLimitRaw','kpRaw','kdRaw','kiRaw']
def regs(s):return {row['id']:{key:row.get(key) for key in fields} for row in s['joints']['data']['servos']}
report=dict(before=before,samples=samples,result=result,after=after,comparison=comparison,registersUnchanged=regs(before)==regs(after),calibrationUnchanged=before['calibration']==after['calibration'],rawDerivation='Fresh shadow starts filtered action at zero; rawTarget=HOME+(filteredTarget-HOME)/0.55. Approximate float reconstruction before target saturation, not future movement prediction.',hardwareCommandsSent=0)
directory=root/'docs/实测记录/附件/HAT/2026-10-04/current-smoothed-targets';directory.mkdir(parents=True,exist_ok=True)
filename=datetime.now().strftime('%H%M%S')+'.json';(directory/filename).write_text(json.dumps(report),encoding='utf-8')
print(json.dumps(dict(file=str(directory/filename),commandCount=result.get('commandCount'),registersUnchanged=report['registersUnchanged'],rows=comparison)))
