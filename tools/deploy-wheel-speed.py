"""Publish hashed frontend assets and switch index last; never restart control."""
from pathlib import Path
import subprocess,json,hashlib,urllib.request,time
root=Path(__file__).resolve().parents[1]
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
stage='/home/radxa/viewer-layout-'+str(int(time.time()))
subprocess.run(['ssh',*opts,host,'mkdir -p '+stage+'/assets'],check=True)
files=[root/'frontend/dist/index.html',*list((root/'frontend/dist/assets').glob('*'))]
for file in files:subprocess.run(['scp',*opts,str(file),host+':'+stage+'/'+file.relative_to(root/'frontend/dist').as_posix()],check=True)
script="""from pathlib import Path
import shutil,os,sys
stage=Path(sys.argv[1]);dist=Path('/home/radxa/microduck-observer/current/frontend/dist');backup=stage/'index.before.html';shutil.copy2(dist/'index.html',backup)
for file in (stage/'assets').iterdir():shutil.copy2(file,dist/'assets'/file.name)
shutil.copy2(stage/'index.html',dist/'index.layout-next');os.replace(dist/'index.layout-next',dist/'index.html')
print('Frontend installed; control service was not restarted')
"""
local=root/'deploy/viewer-layout-install.py';local.write_text(script,encoding='utf-8')
subprocess.run(['scp',*opts,str(local),host+':'+stage+'/install.py'],check=True)
password=next(line.removeprefix('Password: ') for line in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if line.startswith('Password: '))
r=subprocess.run(['ssh',*opts,host,"sudo -S -p '' python3 "+stage+'/install.py '+stage],input=password+'\n',text=True,capture_output=True);print(r.stdout,r.stderr);r.check_returncode()
out={}
for file in files:
 url='http://192.168.31.186:8877/'+file.relative_to(root/'frontend/dist').as_posix();data=urllib.request.urlopen(url,timeout=5).read();assert data==file.read_bytes();out[url]=hashlib.sha256(data).hexdigest()
evidence=root/'docs/实测记录/附件/平台/2026-10-08/wheel-speed';evidence.mkdir(parents=True,exist_ok=True);(evidence/'verification.json').write_text(json.dumps(out),encoding='utf-8');print('Verified',len(out),'resources')
