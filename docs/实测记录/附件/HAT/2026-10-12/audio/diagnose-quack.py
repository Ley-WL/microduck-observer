from pathlib import Path
import subprocess,json
base=Path(__file__).resolve().parent
password=next(s.removeprefix('Password: ') for s in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if s.startswith('Password: '))
ssh=['ssh','-p','22','-o','BatchMode=yes','-o','ConnectTimeout=5','-o','HostKeyAlias=192.168.31.193','radxa@192.168.31.187']
command="sudo -S -p '' amixer -c aic3104 scontents"
r=subprocess.run([*ssh,command],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
(base/'quack-current-mixer.txt').write_text(r.stdout+r.stderr,encoding='utf-8')
sections=r.stdout.split('Simple mixer control')
print('\n'.join(s for s in sections if s.startswith(" 'PCM'") or s.startswith(" 'Line'") or s.startswith(" 'Right Line Mixer DACR1'")))
print('exit',r.returncode)
