"""Exclusive read-only serial diagnostics; never writes servo registers."""
import json, subprocess, time
from pathlib import Path
import serial

ids=[10,11,12,13,14,20,21,22,23,24,30,31,32,33,34]
out={'phases':[]}
def system(cmd):
    r=subprocess.run(cmd,capture_output=True,text=True)
    return {'code':r.returncode,'stdout':r.stdout,'stderr':r.stderr}
out['kernelBefore']=system(['cat','/proc/tty/driver/serial'])
out['journal']=system(['journalctl','-u','microduck-observer','-n','35','--no-pager'])
out['owners']=system(['fuser','/dev/ttyS2'])
s=None
try:
    subprocess.run(['systemctl','stop','microduck-observer'],check=True)
    s=serial.Serial('/dev/ttyS2',1000000,timeout=.002,exclusive=True)
    def request(name,payload,wait=.1):
        s.reset_input_buffer(); packet=b'\xff\xff'+bytes(payload)+bytes([~sum(payload)&255])
        start=time.monotonic();s.write(packet);raw=bytearray(); chunks=[]
        while time.monotonic()-start<wait:
            chunk=s.read(max(1,s.in_waiting))
            if chunk:raw.extend(chunk);chunks.append([round((time.monotonic()-start)*1000,3),len(chunk)])
        frames=[];buf=bytearray(raw)
        while len(buf)>=4:
            if buf[:2]!=b'\xff\xff' or not 2<=buf[3]<=64:del buf[0];continue
            n=buf[3]+4
            if len(buf)<n:break
            f=bytes(buf[:n]);del buf[:n]
            frames.append({'id':f[2],'valid':sum(f[2:])%256==255,'hex':f.hex()})
        out['phases'].append({'name':name,'tx':packet.hex(),'rx':raw.hex(),'chunks':chunks,'frames':frames})
    for order in [ids,ids[::-1],[20,30],[30,20],[31],[20]]:
        request('sync-'+str(order),[254,len(order)+4,0x82,40,31,*order])
    for sid in ids:
        request('individual-'+str(sid),[sid,4,2,40,31])
    for sid in ids:
        request('ping-'+str(sid),[sid,2,1])
        request('identity-'+str(sid),[sid,4,2,0,6])
        request('torque-byte-'+str(sid),[sid,4,2,40,1])
    for order in [ids,ids[::-1]]:
        request('sync-final-'+str(order),[254,len(order)+4,0x82,40,31,*order])
finally:
    if s:s.close()
    subprocess.run(['systemctl','start','microduck-observer'],check=True)
    out['kernelAfter']=system(['cat','/proc/tty/driver/serial'])
    Path('/home/radxa/feedback-loss-readonly.json').write_text(json.dumps(out),encoding='utf-8')
for p in out['phases']:
    print(p['name'],[(f['id'],f['valid']) for f in p['frames']],len(bytes.fromhex(p['rx'])))
