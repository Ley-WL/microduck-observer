from pathlib import Path
import subprocess,shlex
here=Path(__file__).resolve().parent
root=here.parents[5]
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','UserKnownHostsFile='+str(root/'deploy/staging/new-board-known-hosts')]
code='''import subprocess,pathlib,json,time,urllib.request
def cmd(a):return subprocess.run(a,capture_output=True,text=True).stdout
out={'bootId':pathlib.Path('/proc/sys/kernel/random/boot_id').read_text().strip(),'uartBefore':pathlib.Path('/proc/tty/driver/serial').read_text()}
out['service']=cmd(['systemctl','show','microduck-observer','-p','MainPID','-p','NRestarts'])
out['owners']=cmd(['fuser','/dev/ttyS2'])
out['kernelRelevant']='\\n'.join(line for line in cmd(['dmesg']).splitlines() if any(word in line.lower() for word in ['fe660000','ttys2','uart2','overrun','brown','under-voltage','pinctrl']))
out['pinmux']=[]
for p in pathlib.Path('/sys/kernel/debug/pinctrl').glob('*/pinmux-pins'):
    lines=[line for line in p.read_text().splitlines() if line.startswith(('pin 24 ','pin 25 ')) or 'uart2' in line]
    if lines:out['pinmux'].append({'path':str(p),'lines':lines})
time.sleep(10)
out['uartAfter']=pathlib.Path('/proc/tty/driver/serial').read_text()
out['snapshotAfter']=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot'))['joints']
out['hardwareMotion']=False
print(json.dumps(out,indent=2))
'''
password=next(v.removeprefix('Password: ') for v in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if v.startswith('Password: '))
r=subprocess.run(['ssh',*opts,'radxa@192.168.31.230',"sudo -S -p '' python3 -c "+shlex.quote(code)],input=password+'\n',text=True,capture_output=True)
(here/'kernel-read.json').write_text(r.stdout,encoding='utf-8')
print(r.stdout,r.stderr);r.check_returncode()
