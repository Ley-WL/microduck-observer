"""Read-only baud discovery after fresh reboot and whole-bus no-response.

No writes unless actual configuration can be validated by a separate next step.
This guard avoids an unverified/mixed baud or unintended persistent write.
"""
import importlib.util,json,os,subprocess,fcntl,urllib.request,time
from pathlib import Path
spec=importlib.util.spec_from_file_location('baud','/home/radxa/servo-baud-sweep-20261013.py')
b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)
out=Path('/home/radxa/servo-baud-rescue-20261013');out.mkdir(exist_ok=True)
assert not (out/'result.json').exists(),'Do not repeat this diagnosis'
s=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
assert s['system']['data']['servoControl']['state'] in ('idle','failed','disabled')
(out/'before-snapshot.json').write_text(json.dumps(s,indent=2))
r={'hardwareMotion':False,'writes':False,'kernelBootId':Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
   'uptime':Path('/proc/uptime').read_text(),'serialBefore':b.serialcount(),'requests':[]}
for command,key in [(['journalctl','-k','-b','--no-pager','-n','100'],'kernelCurrent'),
                    (['journalctl','-b','-1','--no-pager','-n','100'],'previousBoot'),
                    (['journalctl','-u','microduck-observer','--no-pager','-n','80'],'serviceJournal'),
                    (['systemctl','show','microduck-observer','-p','NRestarts','-p','MainPID','-p','Result','-p','ExecMainStartTimestamp'],'serviceBefore')]:
 v=subprocess.run(command,capture_output=True,text=True);r[key]={'exit':v.returncode,'text':v.stdout+v.stderr}
fd=None;old=None
try:
 subprocess.run(['systemctl','stop','microduck-observer'],check=True)
 assert not subprocess.run(['fuser','/dev/ttyS2'],capture_output=True,text=True).stdout.strip()
 fd=os.open('/dev/ttyS2',os.O_RDWR|os.O_NOCTTY|os.O_NONBLOCK)
 old=bytearray(44);fcntl.ioctl(fd,0x802c542a,old,True)
 for rate in b.RATES:
  b.ratesetting(fd,rate)
  for sid in [30,34]:
   for trial in range(3):
    row=b.exchange(fd,b.packet(sid,2,[5,35]),budget=.12)
    row.update(baud=rate,id=sid,trial=trial);r['requests'].append(row)
  (out/'progress.json').write_text(json.dumps(r,indent=2))
 r['serialAfterDiscovery']=b.serialcount()
 r['validConfigReplies']=[{'baud':v['baud'],'id':v['id'],'data':f['data']} for v in r['requests'] for f in v['frames'] if f['id']==v['id'] and f['error']==0 and len(f['data'])==35]
finally:
 if fd is not None:
  if old is not None:fcntl.ioctl(fd,0x402c542b,old)
  os.close(fd)
 subprocess.run(['systemctl','start','microduck-observer'],check=True)
 r['serviceRestored']=True;r['serialAfter']=b.serialcount()
 (out/'result.json').write_text(json.dumps(r,indent=2))
print(json.dumps({'kernelBootId':r['kernelBootId'],'validConfigReplies':r.get('validConfigReplies'),
 'totalRequests':len(r['requests']),'rxBytes':sum(len(v['rxHex'])//2 for v in r['requests']),
 'serviceRestored':r['serviceRestored'],'writes':False}),flush=True)
