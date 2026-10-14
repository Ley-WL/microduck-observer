from pathlib import Path
import subprocess,shlex
here=Path(__file__).resolve().parent
root=here.parents[5]
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2',
      '-o','UserKnownHostsFile='+str(root/'deploy/staging/new-board-known-hosts')]
host='radxa@192.168.31.230'
remote='/home/radxa/capture-after-r33.py'
subprocess.run(['scp',*opts,str(here/'capture-after-r33.py'),host+':'+remote],check=True)
password=next(v.removeprefix('Password: ') for v in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if v.startswith('Password: '))
result=subprocess.run(['ssh',*opts,host,"sudo -S -p '' python3 "+shlex.quote(remote)],input=password+'\n',text=True,capture_output=True)
(here/'after-r33-capture.log').write_text(result.stdout+result.stderr,encoding='utf-8')
print(result.stdout,result.stderr);result.check_returncode()
