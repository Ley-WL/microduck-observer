import json,hashlib
from pathlib import Path
report={}
for p in Path('.').glob('*/gliding.json'):
 g=json.loads(p.read_text());r=json.loads((p.parent/'results.json').read_text());phases={}
 for phase in ['hold','glide','reverse_turn','centered','deadzone_crossing','release']:
  w=[w for w in g['worlds'] if w['phase']==phase and w['survived']]
  phases[phase]=dict(n=len(w),less10=sum(z['directedDegrees']<10 for z in w),wrongBeyond5=sum(z['directedDegrees']< -5 for z in w),min=min(z['directedDegrees'] for z in w),max=max(z['directedDegrees'] for z in w))
 report[p.parent.name]=dict(worlds=len(r['worlds']),falls=sum(not w['survived'] for w in r['worlds']),elapsedSeconds=g['elapsedSeconds'],peakRssKiB=g['peakRssKiB'],sourceHashesVerified=r['sourceHashesVerified'],phases=phases)
Path('summary.json').write_text(json.dumps(report,indent=2))
print(json.dumps(report,indent=2))
