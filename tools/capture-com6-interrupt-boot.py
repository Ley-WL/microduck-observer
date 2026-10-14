import serial,time
from pathlib import Path
folder=Path(__file__).resolve().parents[1]/'docs/实测记录/附件/主板/2026-10-13/com6-interrupt'
folder.mkdir(exist_ok=True);p=serial.Serial('COM6',1500000,timeout=.03,xonxoff=False,rtscts=False,dsrdtr=False);p.dtr=False;p.rts=False
print('COM6 OPEN 1500000; boot interrupt armed',flush=True)
tail=b'';until=0;began=time.monotonic()
with (folder/'boot.raw').open('ab',buffering=0) as f:
 while time.monotonic()-began<600:
  d=p.read(2048)
  if d:
   f.write(d);tail=(tail+d)[-8192:];print(d.decode('utf-8',errors='replace'),end='',flush=True)
   if b'SPSR = 0x3c9' in tail and not until:until=time.monotonic()+8
  if time.monotonic()<until:p.write(b' ')
p.close()
