import gc
import json,time,subprocess,urllib.request,serial,collections,gzip
from pathlib import Path
IDS=[10,11,12,13,14,20,21,22,23,24,30,31,32,33,34]
def snapshot():return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
def packet(id,op,args):
 b=bytes([id,len(args)+2,op,*args]);return b'\xff\xff'+b+bytes([~sum(b)&255])
def signed(x):return -(x&32767) if x&32768 else x
def read(tx,ids):
 address=tx[5];length=tx[6]
 stale=port.read(port.in_waiting);port.write(tx);start=time.monotonic();raw=bytearray();pending=bytearray();rows={};bad=0;discard=0
 while time.monotonic()-start<.03:
  data=port.read(max(1,min(port.in_waiting,1024)));raw.extend(data);pending.extend(data)
  while len(pending)>=4:
   if pending[:2]!=b'\xff\xff' or not 2<=pending[3]<=64:del pending[0];discard+=1;continue
   size=pending[3]+4
   if len(pending)<size:break
   f=bytes(pending[:size])
   if sum(f[2:])%256!=255:bad+=1;del pending[0];discard+=1;continue
   del pending[:size]
   if f[2] in ids and len(f)==length+6:
    d=f[5:-1];word=lambda n:int.from_bytes(d[n:n+2],'little')
    rows[f[2]]={'position':signed(word(56-address)),'voltage':d[62-address]/10,'currentRaw':signed(word(69-address)),'fault':d[65-address]|f[4]}
    if address==40:rows[f[2]]['torque']=d[0]
  if all(id in rows for id in ids):break
 return {'txHex':tx.hex(),'rxHex':raw.hex(),'residualHex':stale.hex(),'pendingHex':pending.hex(),'rows':rows,'missing':[id for id in ids if id not in rows],'checksumErrors':bad,'discarded':discard,'readMs':(time.monotonic()-start)*1000}
out={'before':snapshot(),'restoreReason':'restore servo23 torque0 after unexpected enable during current-target write test'}
port=None;stopped=False
try:
 assert out['before']['system']['data']['servoControl']['state'] not in ('policy','moving','preflight','disabling')
 subprocess.run(['systemctl','stop','microduck-observer'],check=True);stopped=True
 port=serial.Serial('/dev/ttyS2',1000000,timeout=.0005,exclusive=True)
 out['busBefore']=read(packet(254,0x82,[40,31,*IDS]),IDS)
 assert not out['busBefore']['missing']
 tx=packet(254,0x83,[40,1,23,0]);port.write(tx);out['restoreTxHex']=tx.hex();time.sleep(.02)
 out['busAfter']=read(packet(254,0x82,[40,31,*IDS]),IDS)
 assert not out['busAfter']['missing'] and out['busAfter']['rows'][23]['torque']==0
finally:
 if port:port.close()
 if stopped:subprocess.run(['systemctl','start','microduck-observer'],check=True)
 Path('/home/radxa/single23-restore-torque.json').write_text(json.dumps(out))
print(json.dumps({'restored23':out.get('busAfter',{}).get('rows',{}).get(23)}))
