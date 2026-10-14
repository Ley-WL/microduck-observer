"""Rollback unfinished VOLATILE 500k trial. Never unlock persistence or move."""
import importlib.util,json,os,termios,time,subprocess,urllib.request
from pathlib import Path
spec=importlib.util.spec_from_file_location('b','/home/radxa/servo-baud-sweep-20261013.py')
b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
prior=json.loads(Path('/home/radxa/servo-baud500-native-20261013/result.json').read_text())
assert not prior['committedIds'] and not prior['accepted']
out=Path('/home/radxa/servo-baud-restore-all-20261013');out.mkdir(exist_ok=True)
assert not (out/'result.json').exists()
s=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5));assert s['system']['data']['servoControl']['state'] in ('idle','failed','disabled')
r={'persistentWrites':False,'hardwareMotion':False,'readbacks':{},'writes':[]};fd=None
def write(address,value):
 assert (address,value) in [(55,1),(6,0)]
 v=[254,4,3,address,value];tx=bytes([255,255,*v,(~sum(v))&255])
 os.write(fd,tx);termios.tcdrain(fd);time.sleep(.03);r['writes'].append({'address':address,'value':value})
try:
 subprocess.run(['systemctl','stop','microduck-observer'],check=True)
 assert not subprocess.run(['fuser','/dev/ttyS2'],capture_output=True,text=True).stdout.strip()
 fd=os.open('/dev/ttyS2',os.O_RDWR|os.O_NOCTTY|os.O_NONBLOCK)
 for _ in range(3):
  for rate in (500000,1000000):
   b.ratesetting(fd,rate)
   for _ in range(3):write(55,1)
   write(6,0)
 b.ratesetting(fd,1000000)
 for sid in b.IDS:
  try:
   cfg=b.readreg(fd,sid,5,35,20);lock=b.readreg(fd,sid,55,attempts=20)
   r['readbacks'][str(sid)]={'baudIndex':cfg[1],'lock':lock[0]}
  except Exception as e:r['readbacks'][str(sid)]={'error':repr(e)}
finally:
 if fd is not None:os.close(fd)
 subprocess.run(['systemctl','start','microduck-observer'],check=True)
 r['restoredAll15']=len(r['readbacks'])==15 and all(v.get('baudIndex')==0 and v.get('lock')==1 for v in r['readbacks'].values())
 r['serviceRestored']=True;(out/'result.json').write_text(json.dumps(r,indent=2))
print(json.dumps(r),flush=True)
