from pathlib import Path
import subprocess,shlex
here=Path(__file__).resolve().parent;root=here.parents[5]
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','UserKnownHostsFile='+str(root/'deploy/staging/new-board-known-hosts')]
host='radxa@192.168.31.230'
remote='/home/radxa/internal-servo-loop-20261013.py'
subprocess.run(['scp',*opts,str(here/'internal-loop.py'),host+':'+remote],check=True)
password=next(v.removeprefix('Password: ') for v in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if v.startswith('Password: '))
r=subprocess.run(['ssh',*opts,host,"sudo -S -p '' python3 "+shlex.quote(remote)],input=password+'\n',text=True,capture_output=True)
(here/'internal-loop.log').write_text(r.stdout+r.stderr,encoding='utf-8')
print(r.stdout,r.stderr);r.check_returncode()
