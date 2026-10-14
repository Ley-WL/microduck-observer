"""Isolated ping/register-read only, with service restoration in finally."""
import os,json,termios,select,time,subprocess,urllib.request
from pathlib import Path

out=Path('/home/radxa/servo-independent-read-20261013')
out.mkdir(exist_ok=True)
assert not (out/'result.json').exists(),'Preserve existing independent diagnostic'
snapshot=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
assert snapshot['system']['data']['servoControl']['state'] in ('idle','failed','disabled'),'Do not interrupt an active robot'
def counter():return Path('/proc/tty/driver/serial').read_text()
report=dict(baud=1000000,hardwareMotion=False,onlyInstructions=[1,2],serialBefore=counter(),requests=[])
def packet(servo,instruction,params):
    data=[255,255,servo,len(params)+2,instruction,*params]
    return bytes([*data,(~sum(data[2:]))&255])
fd=None;old=None
try:
    subprocess.run(['systemctl','stop','microduck-observer'],check=True)
    owners=subprocess.run(['fuser','/dev/ttyS2'],capture_output=True,text=True)
    assert not owners.stdout.strip(),'UART must have no other owner'
    fd=os.open('/dev/ttyS2',os.O_RDWR|os.O_NOCTTY|os.O_NONBLOCK)
    old=termios.tcgetattr(fd);settings=termios.tcgetattr(fd)
    settings[:4]=[0,0,termios.CS8|termios.CLOCAL|termios.CREAD,0]
    settings[4]=settings[5]=termios.B1000000
    settings[6][termios.VMIN]=0;settings[6][termios.VTIME]=0
    termios.tcsetattr(fd,termios.TCSANOW,settings)
    time.sleep(.1)
    for servo in (10,30,34):
        for instruction,params in [(1,[]),(2,[56,15])]:
            termios.tcflush(fd,termios.TCIFLUSH)
            tx=packet(servo,instruction,params);started=time.monotonic();os.write(fd,tx)
            rx=bytearray()
            while time.monotonic()-started<.2:
                ready,_,_=select.select([fd],[],[],.01)
                if ready:
                    data=os.read(fd,1024);rx.extend(data)
            report['requests'].append(dict(id=servo,instruction=instruction,txHex=tx.hex(),rxHex=rx.hex(),rxBytes=len(rx),elapsedMs=(time.monotonic()-started)*1000))
    report['serialAfterRaw']=counter()
finally:
    if fd is not None:
        if old is not None:termios.tcsetattr(fd,termios.TCSANOW,old)
        os.close(fd)
    subprocess.run(['systemctl','start','microduck-observer'],check=True)
    for _ in range(40):
        try:
            after=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=2))
            report['serviceRestored']=True;report['onlineAfter']=sum(r['online'] for r in after['joints']['data']['servos'])
            report['controlAfter']=after['system']['data']['servoControl'];break
        except Exception:time.sleep(.25)
    else:report['serviceRestored']=False
    (out/'result.json').write_text(json.dumps(report,indent=2))
print(json.dumps(report))
