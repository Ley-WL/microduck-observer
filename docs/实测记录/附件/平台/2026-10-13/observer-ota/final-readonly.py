import json,hashlib,urllib.request,subprocess
from pathlib import Path
root=Path('/home/radxa/microduck-observer');old=root/'releases/runtime-20261012-224418';new=(root/'current').resolve()
files={str(p.relative_to(old)):hashlib.sha256(p.read_bytes()).hexdigest() for p in old.rglob('*') if p.is_file() and str(p.relative_to(old)) not in ['bin/microduck-observer','ota.json']}
assert all(hashlib.sha256((new/p).read_bytes()).hexdigest()==sha for p,sha in files.items())
s=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5));rows=s['joints']['data']['servos'];assert len(rows)==15 and all(r['online'] and r['torque']==0 for r in rows)
status=json.loads(Path('/var/lib/microduck-observer/ota/status.json').read_text());assert status['phase']=='success'
r=dict(release=str(new),version=s['system']['data']['resources']['board']['serviceVersion'],unchangedReleaseAssetCount=len(files),allServosUnloaded=True,hardwareMotion=False,ota=status,maskedServices={v:subprocess.run(['systemctl','is-enabled',v],capture_output=True,text=True).stdout.strip() for v in ['tofd','rsyslog']})
print(json.dumps(r));assert all(v=='masked' for v in r['maskedServices'].values())
