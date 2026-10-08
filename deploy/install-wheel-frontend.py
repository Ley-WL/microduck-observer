import json,tarfile,hashlib,shutil,os,urllib.request
from pathlib import Path
root=Path('/home/radxa/microduck-observer/current').resolve()
dist=root/'frontend/dist'
stage=Path('/home/radxa/wheel-frontend-20261008')
assert not stage.exists()
stage.mkdir()
with tarfile.open('/home/radxa/wheel-frontend.tar.gz') as a:a.extractall(stage,filter='data')
backup=dist/'index.before-wheel-20261008.html'
assert not backup.exists()
shutil.copy2(dist/'index.html',backup)
# Retain previous assets so open clients continue to work; publish entry point last.
for f in stage.rglob('*'):
 if f.is_file() and f.name!='index.html':
  target=dist/f.relative_to(stage);target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(f,target)
shutil.copy2(stage/'index.html',dist/'index.wheel-next')
os.replace(dist/'index.wheel-next',dist/'index.html')
checks={}
for f in stage.rglob('*'):
 if f.is_file():
  name=str(f.relative_to(stage));data=urllib.request.urlopen('http://127.0.0.1:8877/'+('' if name=='index.html' else name),timeout=5).read()
  checks[name]=hashlib.sha256(data).hexdigest()==hashlib.sha256(f.read_bytes()).hexdigest()
if not all(checks.values()):shutil.copy2(backup,dist/'index.html');raise RuntimeError('HTTP hash mismatch; restored entry')
result={'release':str(root),'backup':str(backup),'httpHashesMatch':checks,'serviceRestarted':False,'motorCommands':0}
Path('/home/radxa/wheel-frontend-deployment.json').write_text(json.dumps(result,indent=2))
print(json.dumps(result))
