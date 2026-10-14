from pathlib import Path
import subprocess,shlex

here=Path(__file__).resolve().parent
root=here.parents[5]
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','UserKnownHostsFile='+str(root/'deploy/staging/new-board-known-hosts')]
host='radxa@192.168.31.230'
script=here/'install-overlays.py'
remote='/home/radxa/audio-repair-20261013/install-overlays.py'
subprocess.run(['scp',*opts,str(script),host+':'+remote],check=True)
password=next(v.removeprefix('Password: ') for v in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if v.startswith('Password: '))
result=subprocess.run(['ssh',*opts,host,"sudo -S -p '' python3 "+shlex.quote(remote)],input=password+'\n',text=True,capture_output=True)
(here/'install.log').write_text(result.stdout+result.stderr,encoding='utf-8')
print(result.stdout,result.stderr)
result.check_returncode()
