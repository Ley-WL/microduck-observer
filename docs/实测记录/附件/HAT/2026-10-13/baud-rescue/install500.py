"""Conditional 500k native-service trial and persistence; no motion commands.

Only persist baud if 30s production telemetry is complete, kernel framing error
delta zero and observed native scanning stays >=50Hz. Preserve all other assets
and configuration. Otherwise revert binary, host baud, servo baud and service.
"""
import asyncio,fcntl,hashlib,importlib.util,json,os,shutil,subprocess,termios,time,urllib.request
from pathlib import Path
import ws_reader as websockets
spec=importlib.util.spec_from_file_location('b','/home/radxa/servo-baud-sweep-20261013.py')
b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
out=Path('/home/radxa/servo-baud500-native-20261013');out.mkdir(exist_ok=True)
assert not (out/'result.json').exists(),'Do not repeat installation'
current=Path('/home/radxa/microduck-observer/current');previous=current.resolve()
release=current.parent/'releases/runtime-20261013-baud500'
assert not release.exists()
runtime=Path('/run/systemd/system/microduck-observer.service.d/90-servo-baud.conf')
persistent=Path('/etc/systemd/system/microduck-observer.service.d/90-servo-baud.conf')
assert not runtime.exists() and not persistent.exists()
snapshot=lambda:json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=4))
s=snapshot();assert s['system']['data']['servoControl']['state'] in ('idle','disabled','failed')
def manifest(p):return {str(f.relative_to(p)):hashlib.sha256(f.read_bytes()).hexdigest() for f in p.rglob('*') if f.is_file()}
assets=manifest(previous);protected=manifest(Path('/var/lib/microduck-observer'))
shutil.copytree(previous,release,symlinks=False)
shutil.copy2('/home/radxa/microduck-observer-baud-new',release/'bin/microduck-observer')
os.chmod(release/'bin/microduck-observer',0o755)
assert {k:v for k,v in manifest(release).items() if k!='bin/microduck-observer'}=={k:v for k,v in assets.items() if k!='bin/microduck-observer'}
r={'hardwareMotion':False,'baud':500000,'previousRelease':str(previous),'release':str(release),
   'assetsBefore':assets,'protectedBefore':protected,'committedIds':[],'accepted':False,'lastConfig':{}}
def point(p):
 tmp=current.with_name('current.baud-staging');tmp.symlink_to(p,target_is_directory=True);os.replace(tmp,current)
def uart():return b.serialcount()
def write(fd,sid,address,value):
 assert address in (6,55) and (address!=55 or value in (0,1))
 data=[sid,4,3,address,value];tx=bytes([255,255,*data,(~sum(data))&255])
 assert os.write(fd,tx)==len(tx);termios.tcdrain(fd);time.sleep(.02)
def openbus(rate):
 assert not subprocess.run(['fuser','/dev/ttyS2'],capture_output=True,text=True).stdout.strip()
 fd=os.open('/dev/ttyS2',os.O_RDWR|os.O_NOCTTY|os.O_NONBLOCK);b.ratesetting(fd,rate);return fd
def setbaud(fd,rateindex,persist=False):
 for sid in b.IDS:
  cfg=b.readreg(fd,sid,5,35,12);ram=b.readreg(fd,sid,40,16,12)
  assert ram[0]==0 and ram[15]==1
  r['lastConfig'][str(sid)]={'config5to39':cfg,'ram40to55':ram}
  if persist:
   r['committedIds'].append(sid);write(fd,sid,55,0);assert b.readreg(fd,sid,55)==[0]
  write(fd,sid,6,rateindex)
  assert b.readreg(fd,sid,6)==[rateindex]
  if persist:write(fd,sid,55,1);assert b.readreg(fd,sid,55)==[1]
async def monitor():
 summary={'serialBefore':uart(),'samples':0,'complete':0,'checksum':0,'observedHz':[],'maxReadMs':0}
 start=time.monotonic();seen=set()
 async with websockets.connect('ws://127.0.0.1:8877/api/v1/stream') as ws:
  await ws.send(json.dumps({'type':'subscribe','topics':{'joints':100,'system':1}}))
  with (out/'native-frames.jsonl').open('w') as f:
   while time.monotonic()-start<30:
    v=json.loads(await asyncio.wait_for(ws.recv(),4));f.write(json.dumps(v)+'\n');f.flush()
    if v.get('type')!='sample':continue
    if v.get('topic')=='system':assert v['data']['servoControl']['state'] in ('idle','disabled','failed'),'External motion; stop verification'
    if v.get('topic')!='joints' or v['seq'] in seen:continue
    seen.add(v['seq']);d=v['data']['diagnostics'];rows=v['data']['servos']
    summary['samples']+=1;summary['complete']+=len(rows)==15 and all(x['online'] and x.get('torque')==0 for x in rows)
    summary['checksum']+=d.get('checksumErrors',0);summary['maxReadMs']=max(summary['maxReadMs'],d['elapsedMs'])
    if time.monotonic()-start>=3 and d.get('observedScanHz') is not None:summary['observedHz'].append(d['observedScanHz'])
 summary['seconds']=time.monotonic()-start;summary['serialAfter']=uart();return summary
fd=None
try:
 subprocess.run(['systemctl','stop','microduck-observer'],check=True)
 fd=openbus(1000000)
 # Direct volatile switch allowed by user; previous audit established lock=1.
 for _ in range(3):write(fd,254,55,1)
 write(fd,254,6,1);b.ratesetting(fd,500000)
 for sid in b.IDS:
  cfg=b.readreg(fd,sid,5,35,20);ram=b.readreg(fd,sid,40,16,20)
  assert cfg[1]==1 and ram[0]==0 and ram[15]==1
  r['lastConfig'][str(sid)]={'config5to39':cfg,'ram40to55':ram}
 os.close(fd);fd=None
 runtime.parent.mkdir(parents=True,exist_ok=True);runtime.write_text('[Service]\nEnvironment=MICRODUCK_SERVO_BAUD=500000\n')
 point(release);subprocess.run(['systemctl','daemon-reload'],check=True);subprocess.run(['systemctl','start','microduck-observer'],check=True)
 for _ in range(50):
  try:snapshot();break
  except Exception:time.sleep(.2)
 time.sleep(3)
 r['trial']=asyncio.run(monitor());q=r['trial']
 def errors(text):
  import re
  line=next(x for x in text.splitlines() if x.startswith('2:'))
  return {k:int(v) for k,v in re.findall(r'(fe|brk|oe|pe):(\d+)',line)}
 a=errors(q['serialBefore']);z=errors(q['serialAfter']);q['kernelErrorDelta']={k:z.get(k,0)-a.get(k,0) for k in set(a)|set(z)}
 q['minObservedHz']=min(q['observedHz']);q['meanObservedHz']=sum(q['observedHz'])/len(q['observedHz'])
 q.pop('observedHz')
 assert q['samples']>=1000 and q['complete']==q['samples'] and q['checksum']==0 and all(v==0 for v in q['kernelErrorDelta'].values()) and q['minObservedHz']>=50,'Low baud failed completeness/error/50Hz condition'
 subprocess.run(['systemctl','stop','microduck-observer'],check=True);fd=openbus(500000)
 setbaud(fd,1,persist=True)
 for sid in b.IDS:
  cfg=b.readreg(fd,sid,5,35,12);ram=b.readreg(fd,sid,40,16,12)
  assert cfg==r['lastConfig'][str(sid)]['config5to39'] and ram==r['lastConfig'][str(sid)]['ram40to55']
 os.close(fd);fd=None
 persistent.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(runtime,persistent);runtime.unlink()
 subprocess.run(['systemctl','daemon-reload'],check=True);subprocess.run(['systemctl','start','microduck-observer'],check=True)
 assert manifest(Path('/var/lib/microduck-observer'))==protected
 r['accepted']=True;r['persistentBaudConfigured']=True
except Exception as e:
 r['error']=repr(e)
 subprocess.run(['systemctl','stop','microduck-observer'],check=True)
 if fd is None:fd=openbus(500000)
 r['rollbackConfirmed']={}
 for sid in b.IDS:
  ok=False
  for rate in [500000,1000000]:
   b.ratesetting(fd,rate)
   try:
    cfg=b.readreg(fd,sid,5,35,5);lock=b.readreg(fd,sid,55,attempts=5)
    if sid in r['committedIds']:write(fd,sid,55,0);assert b.readreg(fd,sid,55)==[0]
    elif lock!=[1]:write(fd,sid,55,1);assert b.readreg(fd,sid,55)==[1]
    write(fd,sid,6,0);b.ratesetting(fd,1000000);assert b.readreg(fd,sid,6)==[0]
    write(fd,sid,55,1);assert b.readreg(fd,sid,55)==[1];ok=True;break
   except Exception:continue
  r['rollbackConfirmed'][str(sid)]=ok
 os.close(fd);fd=None
 if runtime.exists():runtime.unlink()
 if persistent.exists():persistent.unlink()
 point(previous);subprocess.run(['systemctl','daemon-reload'],check=True);subprocess.run(['systemctl','start','microduck-observer'],check=True)
finally:
 if fd is not None:os.close(fd)
 r['protectedAfter']=manifest(Path('/var/lib/microduck-observer'))
 (out/'result.json').write_text(json.dumps(r,indent=2))
print(json.dumps({k:r.get(k) for k in ['accepted','error','trial','rollbackConfirmed','persistentBaudConfigured']}),flush=True)
