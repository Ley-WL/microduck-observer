import asyncio,json,time,urllib.request,subprocess,sys,collections,statistics
sys.path.insert(0,'/home/radxa/microduck-observer/current')
"""Small stdlib WebSocket client for local read-only deployment measurements."""
import asyncio, base64, hashlib, os, socket, struct
from urllib.parse import urlparse

class connect:
    def __init__(self,url):self.url=url
    async def __aenter__(self):
        p=urlparse(self.url)
        if p.scheme!='ws':raise ValueError('Only plain local ws is supported')
        self.socket=socket.create_connection((p.hostname,p.port or 80),timeout=4)
        key=base64.b64encode(os.urandom(16)).decode()
        request=f'GET {p.path} HTTP/1.1\r\nHost: {p.netloc}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n'
        self.socket.sendall(request.encode());header=b''
        while not header.endswith(b'\r\n\r\n'):
            header+=self._read(1)
            if len(header)>16384:raise ValueError('Oversize handshake')
        expected=base64.b64encode(hashlib.sha1((key+'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest())
        if b' 101 ' not in header.split(b'\r\n')[0] or expected not in header:
            self.socket.close();raise ValueError('Invalid WebSocket handshake')
        return self
    def _read(self,n):
        result=b''
        while len(result)<n:
            chunk=self.socket.recv(n-len(result))
            if not chunk:raise EOFError('WebSocket closed')
            result+=chunk
        return result
    def _send(self,op,payload):
        n=len(payload);mask=os.urandom(4)
        prefix=bytes([0x80|op,0x80|(n if n<126 else 126 if n<65536 else 127)])
        if n>=126:prefix+=struct.pack('!H' if n<65536 else '!Q',n)
        self.socket.sendall(prefix+mask+bytes(v^mask[i%4] for i,v in enumerate(payload)))
    async def send(self,text):self._send(1,text.encode())
    def _receive(self):
        message=b''
        while True:
            head=self._read(2);op=head[0]&15;n=head[1]&127
            if head[1]&128:raise ValueError('Server frame must be unmasked')
            if n==126:n=struct.unpack('!H',self._read(2))[0]
            elif n==127:n=struct.unpack('!Q',self._read(8))[0]
            if n>1048576:raise ValueError('Oversize frame')
            data=self._read(n)
            if op==8:raise EOFError('WebSocket closed')
            if op==9:self._send(10,data);continue
            if op==10:continue
            if op not in (0,1):raise ValueError('Expected text frame')
            message+=data
            if len(message)>1048576:raise ValueError('Oversize message')
            if head[0]&128:return message.decode()
    async def recv(self):return await asyncio.to_thread(self._receive)
    async def __aexit__(self,*args):self.socket.close()

report={}
def cmd(args):
 r=subprocess.run(args,capture_output=True,text=True,timeout=10);return {'code':r.returncode,'out':r.stdout,'err':r.stderr}
for name,args in {'wifiLink':['iw','dev','wlan0','link'],'wifiPowerSave':['iw','dev','wlan0','get','power_save'],'wifiStation':['iw','dev','wlan0','station','dump'],'route':['ip','route'],'service':['systemctl','show','microduck-observer','-p','ActiveState','-p','NRestarts','-p','MainPID','-p','ExecMainStartTimestamp'],'serviceJournal':['journalctl','-u','microduck-observer','--since','30 minutes ago','--no-pager','-n','160'],'kernelJournal':['journalctl','-k','--since','30 minutes ago','--no-pager','-n','100']}.items():report[name]=cmd(args)
lat=[]
for i in range(20):
 start=time.perf_counter();raw=urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5).read();lat.append((time.perf_counter()-start)*1000)
 if i==0:report['snapshot']=json.loads(raw)
 time.sleep(.05)
report['httpLoopbackMs']={'samples':lat,'median':statistics.median(lat),'max':max(lat)}
async def measure():
 counts=collections.Counter();sizes=collections.Counter();ages=collections.defaultdict(list);hb=[];last=None;gaps=[];started=time.perf_counter()
 async with connect('ws://127.0.0.1:8877/api/v1/stream') as ws:
  await ws.send(json.dumps({'type':'subscribe','requestId':'readonly-web-diagnostic','topics':{'pose':50,'imu.orientation':50,'imu.raw':50,'joints':20,'tof':20,'system':1,'logs':None}}))
  while time.perf_counter()-started<20:
   raw=await ws.recv();now=time.perf_counter();m=json.loads(raw);topic=m.get('topic',m['type']);counts[topic]+=1;sizes[topic]+=len(raw.encode())
   if last is not None:gaps.append((now-last)*1000)
   last=now
   if m['type']=='heartbeat':hb.append(now-started)
   if 'ageMs' in m:ages[topic].append(m['ageMs'])
 report['wsLoopback']={'seconds':time.perf_counter()-started,'counts':dict(counts),'bytes':dict(sizes),'heartbeatTimes':hb,'maxReceiveGapMs':max(gaps),'agesMs':{k:{'median':statistics.median(v),'max':max(v)} for k,v in ages.items()}}
asyncio.run(measure())
for line in report['route']['out'].splitlines():
 if line.startswith('default via '):report['gatewayPing']=cmd(['ping','-c','10','-i','.2','-W','1',line.split()[2]]);break
report['afterSnapshot']=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
print(json.dumps(report,ensure_ascii=False))
