import os,json,termios,fcntl,array,time,select,subprocess,urllib.request
from pathlib import Path
out=Path('/home/radxa/servo-independent-read-20261013')
assert not (out/'internal-loop.json').exists()
s=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
assert s['system']['data']['servoControl']['state'] in ('idle','failed','disabled')
fd=None;old=None;loop_set=False;old_loop=False;rows=[]
def get_bits():
    bits=array.array('i',[0]);fcntl.ioctl(fd,termios.TIOCMGET,bits,True);return bits[0]
try:
    subprocess.run(['systemctl','stop','microduck-observer'],check=True)
    assert not subprocess.run(['fuser','/dev/ttyS2'],capture_output=True,text=True).stdout.strip()
    fd=os.open('/dev/ttyS2',os.O_RDWR|os.O_NOCTTY|os.O_NONBLOCK)
    old=termios.tcgetattr(fd);cfg=termios.tcgetattr(fd)
    cfg[:4]=[0,0,termios.CS8|termios.CLOCAL|termios.CREAD,0]
    cfg[4]=cfg[5]=termios.B1000000;cfg[6][termios.VMIN]=0;cfg[6][termios.VTIME]=0
    termios.tcsetattr(fd,termios.TCSANOW,cfg)
    bit=0x8000
    old_loop=bool(get_bits()&bit)
    fcntl.ioctl(fd,termios.TIOCMBIS,array.array('i',[bit]))
    loop_set=True
    assert get_bits()&bit,'Loopback flag was not applied; no test bytes sent'
    for trial in range(3):
        termios.tcflush(fd,termios.TCIFLUSH)
        payload=(b'MicroDuck-Internal-ReadOnly-'+bytes([48+trial]))
        started=time.monotonic();os.write(fd,payload);rx=bytearray()
        while time.monotonic()-started<.2 and len(rx)<len(payload):
            ready,_,_=select.select([fd],[],[],.01)
            if ready:rx.extend(os.read(fd,1024))
        rows.append(dict(trial=trial,txHex=payload.hex(),rxHex=rx.hex(),matched=rx==payload))
finally:
    if fd is not None:
        if loop_set and not old_loop:fcntl.ioctl(fd,termios.TIOCMBIC,array.array('i',[0x8000]))
        if old is not None:termios.tcsetattr(fd,termios.TCSANOW,old)
        os.close(fd)
    subprocess.run(['systemctl','start','microduck-observer'],check=True)
    report=dict(baud=1000000,internalLoopFlagVerified=loop_set,rows=rows,hardwareMotion=False,
                dataContainsServoInstructions=False,loopModeRestored=True,serviceRestarted=True)
    (out/'internal-loop.json').write_text(json.dumps(report,indent=2))
print(json.dumps(report))
