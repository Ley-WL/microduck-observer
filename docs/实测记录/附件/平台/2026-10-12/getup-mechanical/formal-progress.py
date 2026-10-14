from pathlib import Path
import json,datetime
root=Path('/home/leiwu30/microduck-training/getup-mechanical-train-20261012/evidence/mechanical-formal')
r={'observedUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'jobExit':(root/'job.exit').read_text() if (root/'job.exit').exists() else None,'totalPlanned':4800,'conditions':{}}
for kind in ['normal','stress']:
    rows=[json.loads(p.read_text()) for p in (root/kind).glob('*.json') if p.name!='summary.json']
    groups=[json.loads(line) for line in (root/(kind+'.log')).read_text().splitlines() if line.startswith('{')]
    r['conditions'][kind]={'completedTrials':len(rows),'completedGroups':groups}
p=Path('/tmp/getup-formal-progress.json');p.write_text(json.dumps(r,indent=2));print(json.dumps(r))
