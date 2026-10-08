import json,time,subprocess,urllib.request,serial
from pathlib import Path
IDS=[10,11,12,13,14,20,21,22,23,24,30,31,32,33,34]
def snapshot():return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
def crc(data):
 c=0
 for b in data:
  c^=b<<8
  for _ in range(8):c=((c<<1)^0x8005 if c&0x8000 else c<<1)&65535
 return c
def p1(id):
 b=bytes([id,2,1]);return b'\xff\xff'+b+bytes([~sum(b)&255])
def p2(id):
 b=bytes([255,255,253,0,id,3,0,1]);return b+crc(b).to_bytes(2,'little')
assert p2(1).hex()=='fffffd0001030001194e'
out={'readOnly':True,'registerWrites':0,'positionCommands':0,'before':snapshot(),'trials':[]};port=None;stopped=False
try:
 assert out['before']['system']['data']['servoControl']['state'] not in ('policy','moving','preflight','disabling')
 assert all(r['online'] for r in out['before']['joints']['data']['servos'])
 subprocess.run(['systemctl','stop','microduck-observer'],check=True);stopped=True
 port=serial.Serial('/dev/ttyS2',1000000,timeout=.002)
 for repeat in range(3):
  for version,make in [(1,p1),(2,p2)]:
   valid=[]
   for id in IDS:
    port.reset_input_buffer();tx=make(id);port.write(tx);port.flush();raw=bytearray();start=time.monotonic()
    while time.monotonic()-start<.06:raw.extend(port.read(max(1,port.in_waiting)))
    frames=[]
    for i in range(len(raw)):
     if version==1 and raw[i:i+2]==b'\xff\xff' and i+4<=len(raw):
      n=raw[i+3]+4;f=raw[i:i+n]
      if len(f)==n and n>=6 and f[2]==id and sum(f[2:])%256==255:frames.append(f.hex())
     if version==2 and raw[i:i+4]==b'\xff\xff\xfd\x00' and i+7<=len(raw):
      n=int.from_bytes(raw[i+5:i+7],'little')+7;f=raw[i:i+n]
      if len(f)==n and n>=11 and f[4]==id and f[7]==0x55 and crc(f[:-2])==int.from_bytes(f[-2:],'little'):frames.append(f.hex())
    out['trials'].append({'round':repeat+1,'protocol':version,'id':id,'txHex':tx.hex(),'rxHex':raw.hex(),'validFrames':frames})
    if frames:valid.append(id)
   print(json.dumps({'round':repeat+1,'protocol':version,'validIds':valid}),flush=True)
except Exception as e:out['error']=str(e);raise
finally:
 if port:port.close()
 if stopped:
  subprocess.run(['systemctl','start','microduck-observer'],check=True)
  for _ in range(30):
   try:
    s=snapshot()
    if all(r['online'] for r in s['joints']['data']['servos']):out['after']=s;break
   except Exception:pass
   time.sleep(.2)
 Path('/home/radxa/dynamixel-readonly-probe.json').write_text(json.dumps(out))
