from pathlib import Path
import subprocess
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193']
host='radxa@192.168.31.186'
root=Path(__file__).resolve().parent
password=next(x.removeprefix('Password: ') for x in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if x.startswith('Password: '))
subprocess.run(['scp',*opts,str(root/'test-late-replies-board.py'),f'{host}:/home/radxa/'],check=True)
r=subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/test-late-replies-board.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(r.stdout,r.stderr,flush=True)
subprocess.run(['scp',*opts,f'{host}:/home/radxa/late-replies-20261003.json',str(root.parent/'docs/实测记录/附件/HAT/2026-10-03/late-replies.json')],check=True)
r.check_returncode()
