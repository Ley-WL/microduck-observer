import serial,time,json
from pathlib import Path
folder=Path(__file__).resolve().parents[1]/'docs/瀹炴祴璁板綍/闄勪欢/涓绘澘/2026-10-13/com5-new-card-115200'
folder.mkdir(parents=True,exist_ok=True)
port=serial.Serial('COM5',115200,bytesize=8,parity='N',stopbits=1,timeout=.2,xonxoff=False,rtscts=False,dsrdtr=False)
port.dtr=False;port.rts=False
print('COM5 OPEN 115200 8N1 receive-only',flush=True)
(folder/'status.json').write_text(json.dumps({'phase':'listening','port':'COM5','baud':115200}))
began=time.monotonic();count=0
with (folder/'boot.raw').open('ab',buffering=0) as raw,(folder/'boot.txt').open('a',encoding='utf-8') as log:
 while time.monotonic()-began<600:
  data=port.read(8192)
  if data:
   raw.write(data);count+=len(data);text=data.decode('utf-8',errors='replace');log.write(text);log.flush();print(text,end='',flush=True)
port.close();(folder/'status.json').write_text(json.dumps({'phase':'closed','bytes':count}))
