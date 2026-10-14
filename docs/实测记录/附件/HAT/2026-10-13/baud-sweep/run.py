from pathlib import Path
import subprocess,shlex
here=Path(__file__).resolve().parent; root=here.parents[5]
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2',
      '-o','UserKnownHostsFile='+str(root/'deploy/staging/new-board-known-hosts')]
host='radxa@192.168.31.230'
remote='/home/radxa/servo-baud-sweep-20261013.py'
subprocess.run(['scp',*opts,str(here/'sweep.py'),host+':'+remote],check=True)
password=next(v.removeprefix('Password: ') for v in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if v.startswith('Password: '))
p=subprocess.Popen(['ssh',*opts,host,"sudo -S -p '' python3 -u "+shlex.quote(remote)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
p.stdin.write(password+'\n');p.stdin.close()
with (here/'run.log').open('w',encoding='utf-8') as log:
    for line in p.stdout:log.write(line);log.flush();print(line,end='',flush=True)
raise SystemExit(p.wait())
