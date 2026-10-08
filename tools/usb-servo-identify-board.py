import json,time,subprocess,urllib.request,serial,collections
from pathlib import Path
IDS=[10,11,12,13,14,20,21,22,23,24,30,31,32,33,34]
def snapshot():return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
def packet(id,op,args):
 b=bytes([id,len(args)+2,op,*args]);return b'\xff\xff'+b+bytes([~sum(b)&255])
def signed(x):return -(x&32767) if x&32768 else x
def read(tx,ids):
 stale=port.read(port.in_waiting);port.write(tx);port.flush();start=time.monotonic();raw=bytearray();pending=bytearray();rows={};bad=0;discard=0
 while time.monotonic()-start<.03:
  data=port.read(max(1,min(port.in_waiting,1024)));raw.extend(data);pending.extend(data)
  while len(pending)>=4:
   if pending[:2]!=b'\xff\xff' or not 2<=pending[3]<=64:del pending[0];discard+=1;continue
   size=pending[3]+4
   if len(pending)<size:break
   f=bytes(pending[:size])
   if sum(f[2:])%256!=255:bad+=1;del pending[0];discard+=1;continue
   del pending[:size]
   if f[2] in ids and len(f)==37:
    d=f[5:-1];word=lambda n:int.from_bytes(d[n:n+2],'little')
    rows[f[2]]={'torque':d[0],'position':signed(word(16)),'voltage':d[22]/10,'currentRaw':signed(word(29)),'fault':d[25]|f[4]}
  if all(id in rows for id in ids):break
 return {'txHex':tx.hex(),'rxHex':raw.hex(),'residualHex':stale.hex(),'pendingHex':pending.hex(),'rows':rows,'missing':[id for id in ids if id not in rows],'checksumErrors':bad,'discarded':discard,'readMs':(time.monotonic()-start)*1000}
out={'registerWrites':0,'positionCommands':0,'before':snapshot(),'rounds':[]};port=None;stopped=False
try:
 assert out['before']['system']['data']['servoControl']['state'] not in ('policy','moving','preflight','disabling')
 assert True
 subprocess.run(['systemctl','stop','microduck-observer'],check=True);stopped=True
 port=serial.Serial('/dev/serial/by-id/usb-1a86_USB_Single_Serial_5B79076110-if00',1000000,timeout=.0005,exclusive=True)
 for repetition in range(1,2):
  for mode in ['standard-read','hd-sync-read']:
   run={'round':repetition,'mode':mode,'kernelBefore':Path('/proc/tty/driver/serial').read_text(),'frames':[]};out['rounds'].append(run);began=time.monotonic();due=began;times=[]
   for n in range(20):
    time.sleep(max(0,due-time.monotonic()));times.append(time.monotonic());scans=[]
    if mode=='standard-read':
     for id in IDS:scans.append(read(packet(id,2,[40,31]),[id]))
    else:scans.append(read(packet(254,0x82,[40,31,*IDS]),IDS))
    run['frames'].append(scans)
    for scan in scans:
     assert all(r['torque'] in (0,1) for r in scan['rows'].values()),'收到真实状态异常，停止试验'
    due+=.02
    if due<time.monotonic():due=began+(int((time.monotonic()-began)/.02)+1)*.02
   missing=collections.Counter();bad=0;incomplete=0
   for scans in run['frames']:
    ids=[]
    for scan in scans:ids+=scan['missing'];bad+=scan['checksumErrors']>0
    missing.update(ids);incomplete+=bool(ids)
   run['kernelAfter']=Path('/proc/tty/driver/serial').read_text()
   run['summary']={'round':repetition,'mode':mode,'frames':20,'missingFrames':incomplete,'missingResponses':sum(missing.values()),'expectedResponses':300,'missingById':dict(missing),'checksumTransactions':bad,'frameHz':19/(times[-1]-times[0]),'maxFrameGapMs':max(b-a for a,b in zip(times,times[1:]))*1000}
   Path('/home/radxa/usb-servo-identification.json').write_text(json.dumps(out));print(json.dumps(run['summary']),flush=True)
except Exception as e:out['error']=str(e);raise
finally:
 if port:port.close()
 if stopped:
  subprocess.run(['systemctl','start','microduck-observer'],check=True)
  for _ in range(40):
   try:
    s=snapshot()
    if all(r['online'] for r in s['joints']['data']['servos']):out['after']=s;break
   except Exception:pass
   time.sleep(.2)
 Path('/home/radxa/usb-servo-identification.json').write_text(json.dumps(out))
