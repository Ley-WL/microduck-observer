"""User-authorized direct temporary baud rescue; broadcast lock=1 then baud.

Last successful audit confirmed all locks=1. Current no-response prevents fresh
ack validation. Command delivery is therefore UNCONFIRMED until low-rate reads.
Only addresses55=1 and6 are written, never motion or persistence enable.
Failure rolls back on both possible active rates; actual device rollback is
reported unconfirmed if no replies return, requiring servo power cycle.
"""
import importlib.util,json,os,time,fcntl,termios,subprocess,urllib.request
from pathlib import Path
spec=importlib.util.spec_from_file_location('b','/home/radxa/servo-baud-sweep-20261013.py')
b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
out=Path('/home/radxa/servo-direct250-20261013');out.mkdir(exist_ok=True)
assert not (out/'result.json').exists()
prior=json.loads(Path('/home/radxa/servo-baud-load50-20261013/result.json').read_text())
assert prior['registersBefore']==prior['registersAfter']
assert all(v['ram40to55'][-1]==1 for v in prior['registersAfter'].values())
s=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
assert s['system']['data']['servoControl']['state'] in ('idle','failed','disabled')
r={'hardwareMotion':False,'enablePersistenceWritten':False,'lastVerifiedLocksAll1':True,
   'currentLockReadbackUnavailable':True,'requestedBaud':250000,'requests':[],'writes':[]}
def write(fd,address,value):
 assert (address==55 and value==1) or (address==6 and value in (0,2))
 data=[254,4,3,address,value];tx=bytes([255,255,*data,(~sum(data))&255])
 assert os.write(fd,tx)==len(tx);termios.tcdrain(fd);time.sleep(.025)
 r['writes'].append({'address':address,'value':value,'txHex':tx.hex(),'deliveryConfirmed':False})
fd=None;old=None
try:
 subprocess.run(['systemctl','stop','microduck-observer'],check=True)
 assert not subprocess.run(['fuser','/dev/ttyS2'],capture_output=True,text=True).stdout.strip()
 fd=os.open('/dev/ttyS2',os.O_RDWR|os.O_NOCTTY|os.O_NONBLOCK)
 old=bytearray(44);fcntl.ioctl(fd,0x802c542a,old,True);b.ratesetting(fd,1000000)
 for _ in range(3):write(fd,55,1)
 write(fd,6,2);b.ratesetting(fd,250000)
 for sid in b.IDS:
  for trial in range(2):
   row=b.exchange(fd,b.packet(sid,2,[5,35]),.15);row.update(id=sid,trial=trial,baud=250000);r['requests'].append(row)
 r['lowRateConfirmedIds']=sorted({f['id'] for v in r['requests'] for f in v['frames'] if f['error']==0 and len(f['data'])==35 and f['data'][1]==2})
 r['lowRateSuccess']=r['lowRateConfirmedIds']==b.IDS
finally:
 if fd is not None:
  # Roll back at BOTH possible device rates, even after a partial switch.
  for rate in (250000,1000000):
   b.ratesetting(fd,rate)
   for _ in range(3):write(fd,55,1)
   write(fd,6,0)
  b.ratesetting(fd,1000000);r['restorationReadbacks']={}
  for sid in b.IDS:
   try:r['restorationReadbacks'][str(sid)]=b.readreg(fd,sid,6,attempts=2)
   except Exception:r['restorationReadbacks'][str(sid)]=None
  if old is not None:fcntl.ioctl(fd,0x402c542b,old)
  os.close(fd)
 subprocess.run(['systemctl','start','microduck-observer'],check=True);r['serviceRestored']=True
 r['deviceRestorationConfirmed']=all(v==[0] for v in r.get('restorationReadbacks',{}).values()) and len(r.get('restorationReadbacks',{}))==15
 (out/'result.json').write_text(json.dumps(r,indent=2))
print(json.dumps({'lowRateConfirmedIds':r.get('lowRateConfirmedIds'),'lowRateSuccess':r.get('lowRateSuccess'),
 'deviceRestorationConfirmed':r['deviceRestorationConfirmed'],'serviceRestored':True,'hardwareMotion':False}),flush=True)
