import json,hashlib,shutil
from pathlib import Path
root=Path('/home/radxa/microduck-observer/current').resolve()
notices=root/'licenses/microduck-sounds';notices.mkdir(parents=True,exist_ok=True)
for name in ['LICENSE','UPSTREAM.md']:shutil.copyfile('/tmp/'+name,notices/name)
bank=Path('/var/lib/microduck-observer/voice')
m=json.loads((bank/'manifest.json').read_text())
assert all(hashlib.sha256((bank/f'greet_{i}.wav').read_bytes()).hexdigest()==h for i,h in enumerate(m['sha256']))
print(json.dumps({'release':str(root),'manifest':m,'productionCacheHashesVerified':True,'licensesPresent':True}))
