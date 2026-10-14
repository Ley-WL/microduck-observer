from pathlib import Path
import subprocess,json,shlex

here=Path(__file__).resolve().parent
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','HostKeyAlias=192.168.31.193']
code='''import subprocess,json,pathlib
names=['PCM','Line','Line DAC','Right Line Mixer DACR1','Left Line Mixer DACL1','Right Line Mixer Line2R Bypass','Right Line Mixer PGAR Bypass','Left Line Mixer Line2L Bypass','Left Line Mixer PGAL Bypass']
out={}
for name in names:
    r=subprocess.run(['amixer','-c','1','sget',name],capture_output=True,text=True)
    out[name]=dict(exit=r.returncode,output=r.stdout,error=r.stderr)
out['cards']=pathlib.Path('/proc/asound/cards').read_text()
out['voiceFiles']=[p.name for p in pathlib.Path('/var/lib/microduck-observer/voice').glob('*.wav')]
out['serviceAudioLogs']=subprocess.run(['journalctl','-u','microduck-observer','--since','30 minutes ago','--no-pager'],capture_output=True,text=True).stdout[-5000:]
print(json.dumps(out,indent=2))
'''
password=next(v.removeprefix('Password: ') for v in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if v.startswith('Password: '))
r=subprocess.run(['ssh',*opts,'radxa@192.168.31.187',"sudo -S -p '' python3 -c "+shlex.quote(code)],input=password+'\n',text=True,capture_output=True)
(here/'inspection.json').write_text(r.stdout,encoding='utf-8')
print(r.stdout,r.stderr);r.check_returncode()
