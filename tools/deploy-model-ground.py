from pathlib import Path
import base64
import subprocess
import tarfile

root=Path(__file__).resolve().parents[1]
bundle=root/'deploy/staging/model-ground.tar.gz'
with tarfile.open(bundle,'w:gz') as archive:
    archive.add(root/'frontend/dist',arcname='dist')
options=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193']
host='radxa@192.168.31.186'
subprocess.run(['scp',*options,str(bundle),host+':/home/radxa/model-ground.tar.gz'],check=True)
code='''from pathlib import Path
import shutil,tarfile,os,json,urllib.request
r=Path('/home/radxa/microduck-observer')
old=(r/'current').resolve()
new=r/'releases/20261003-model-ground'
assert not new.exists()
shutil.copytree(old,new,symlinks=True)
with tarfile.open('/home/radxa/model-ground.tar.gz') as a: a.extractall(new/'frontend',filter='data')
tmp=r/'current.model-ground'
tmp.symlink_to(new)
os.replace(tmp,r/'current')
print(json.dumps({'old':str(old),'new':str(new),'backendRestarted':False,'calibrationWritten':False}))
'''
encoded=base64.b64encode(code.encode()).decode()
password=next(x.removeprefix('Password: ') for x in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if x.startswith('Password: '))
command="sudo -S -p '' python3 -c \"import base64;exec(base64.b64decode('"+encoded+"'))\""
result=subprocess.run(['ssh',*options,host,command],input=password+'\n',text=True,capture_output=True)
print(result.stdout,result.stderr)
raise SystemExit(result.returncode)
