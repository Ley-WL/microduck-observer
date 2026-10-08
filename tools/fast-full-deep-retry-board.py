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
   if f[2] in ids and len(f)==length+6:
    d=f[5:-1];word=lambda n:int.from_bytes(d[n:n+2],'little')
    rows[f[2]]={'position':signed(word(56-address)),'voltage':d[62-address]/10,'currentRaw':signed(word(69-address)),'fault':d[65-address]|f[4]}
    if address==40:rows[f[2]]['torque']=d[0]
  if all(id in rows for id in ids):break
 return {'txHex':tx.hex(),'rxHex':raw.hex(),'residualHex':stale.hex(),'pendingHex':pending.hex(),'rows':rows,'missing':[id for id in ids if id not in rows],'checksumErrors':bad,'discarded':discard,'readMs':(time.monotonic()-start)*1000}
out={'registerWrites':0,'positionCommands':0,'before':snapshot(),'rounds':[]};port=None;stopped=False
try:
 assert out['before']['system']['data']['servoControl']['state'] not in ('policy','moving','preflight','disabling')
 assert all(r.get('torque',0) in (0,1) for r in out['before']['joints']['data']['servos']), '非法使能字段'
 subprocess.run(['systemctl','stop','microduck-observer'],check=True);stopped=True
 port=serial.Serial('/dev/ttyS2',1000000,timeout=.0005,exclusive=True)
 out['prechecks']=[]
 for trial in range(5):
  out['baseline']=read(packet(254,0x82,[40,31,*IDS]),IDS);out['prechecks'].append(out['baseline'])
  if not out['baseline']['missing']:break
  time.sleep(.05)
 assert not out['baseline']['missing'] and all(r['torque'] in (0,1) for r in out['baseline']['rows'].values())
 for repetition in range(1,11):
  for mode in (['fast','full'] if repetition%2 else ['full','fast']):
   run={'round':repetition,'mode':mode,'kernelBefore':Path('/proc/tty/driver/serial').read_text(),'frames':[]};out['rounds'].append(run);began=time.monotonic();due=began;times=[]
   gc.collect();gc.disable()
   for n in range(3000):
    time.sleep(max(0,due-time.monotonic()));times.append(time.monotonic());scans=[]
    if mode=='standard-read':
     for id in IDS:scans.append(read(packet(id,2,[40,31]),[id]))
    else:scans.append(read(packet(254,0x82,[56,15,*IDS] if mode=='fast' else [40,31,*IDS]),IDS))
    run['frames'].append(scans)
    for scan in scans:
     assert all(r.get('torque',0) in (0,1) and r['fault']==0 and 4<=r['voltage']<=8.4 for r in scan['rows'].values()),'收到真实状态异常，停止试验'
    if (n+1)%750==0:print(json.dumps({'progressRound':repetition,'frames':n+1}),flush=True)
    due+=.02
    if due<time.monotonic():due=began+(int((time.monotonic()-began)/.02)+1)*.02
   gc.enable()
   missing=collections.Counter();bad=0;incomplete=0
   for scans in run['frames']:
    ids=[]
    for scan in scans:ids+=scan['missing'];bad+=scan['checksumErrors']>0
    missing.update(ids);incomplete+=bool(ids)
   run['kernelAfter']=Path('/proc/tty/driver/serial').read_text()
   run['summary']={'round':repetition,'mode':mode,'frames':3000,'missingFrames':incomplete,'missingResponses':sum(missing.values()),'expectedResponses':45000,'missingById':dict(missing),'checksumTransactions':bad,'frameHz':2999/(times[-1]-times[0]),'maxFrameGapMs':max(b-a for a,b in zip(times,times[1:]))*1000}
   with gzip.open('/home/radxa/fast-full-deep-retry-round-'+str(repetition)+'-'+mode+'.json.gz','wt') as f:json.dump(run,f)
   run['failures']=[{'frame':n+1,'scans':scans} for n,scans in enumerate(run['frames']) if any(s['missing'] or s['checksumErrors'] for s in scans)]
   del run['frames']
   Path('/home/radxa/fast-full-deep-retry-summary.json').write_text(json.dumps(out));print(json.dumps(run['summary']),flush=True)
except Exception as e:out['error']=str(e);raise
finally:
 gc.enable()
 if port:
  out['finalBusRead']=read(packet(254,0x82,[40,31,*IDS]),IDS)
  port.close()
 if stopped:
  subprocess.run(['systemctl','start','microduck-observer'],check=True)
  for _ in range(40):
   try:
    s=snapshot()
    if all(r['online'] for r in s['joints']['data']['servos']):out['after']=s;break
   except Exception:pass
   time.sleep(.2)
 Path('/home/radxa/fast-full-deep-retry-summary.json').write_text(json.dumps(out))
