import serial,time,json
from pathlib import Path
folder=Path(__file__).resolve().parents[1]/'docs/实测记录/附件/主板/2026-10-13/com6-auto-baud'
folder.mkdir(parents=True,exist_ok=True)
port=serial.Serial('COM6',1500000,bytesize=8,parity='N',stopbits=1,timeout=.2,xonxoff=False,rtscts=False,dsrdtr=False)
port.dtr=False;port.rts=False
print('COM6 OPEN 1500000 8N1 receive-only',flush=True)
(folder/'status.json').write_text(json.dumps({'phase':'listening','port':'COM6','baud':1500000}))
began=time.monotonic();count=0;tail=b"";switched=False
with (folder/'boot.raw').open('ab',buffering=0) as raw,(folder/'boot.txt').open('a',encoding='utf-8') as log:
 while time.monotonic()-began<600:
  data=port.read(8192)
  if data:
   raw.write(data);count+=len(data);tail=(tail+data)[-8192:]
   if not switched and b'SPSR = 0x3c9' in tail:
    port.baudrate=115200;switched=True;print('\nSWITCH COM6 TO 115200 after BL31',flush=True)
   text=data.decode('utf-8',errors='replace');log.write(text);log.flush();print(text,end='',flush=True)
port.close();(folder/'status.json').write_text(json.dumps({'phase':'closed','bytes':count}))
