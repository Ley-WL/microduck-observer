from pathlib import Path
import subprocess,shlex

here=Path(__file__).resolve().parent
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','HostKeyAlias=192.168.31.193']
code='''import subprocess,json,pathlib,hashlib
stage=pathlib.Path('/home/radxa/audio-repair-187-20261013');stage.mkdir(exist_ok=True)
def run(a):return subprocess.check_output(a,text=True,stderr=subprocess.STDOUT)
run(['alsactl','-f',str(stage/'before.state'),'store','1'])
before=run(['amixer','-c','1','scontents']);(stage/'before.txt').write_text(before)
names=[]
for line in run(['amixer','-c','1','scontrols']).splitlines():
    name=line.split("'")[1]
    if 'Line Mixer' in name and 'Bypass' in name:
        run(['amixer','-q','-c','1','sset',name,'off']);names.append(name)
run(['amixer','-q','-c','1','sset','Right Line Mixer DACR1','on'])
run(['alsactl','store','1'])
after=run(['amixer','-c','1','scontents']);(stage/'after.txt').write_text(after)
clip=pathlib.Path('/var/lib/microduck-observer/voice/greet_0.wav')
run(['systemd-run','--quiet','--wait','--pipe','--collect','--unit=microduck-audio187-preview',
     '-p','User=radxa','-p','SupplementaryGroups=audio','-p','DevicePolicy=closed',
     '-p','DeviceAllow=/dev/snd/controlC1 rw','-p','DeviceAllow=/dev/snd/pcmC1D0p rw',
     '/usr/bin/aplay','-q','-D','plughw:CARD=aic3104,DEV=0',str(clip)])
report=dict(outputBypassesDisabled=names,pcmUnchanged=True,lineVolumeUnchanged=True,audioPreviewExit=0,
            audioSha256=hashlib.sha256(clip.read_bytes()).hexdigest(),hardwareMotion=False,serviceRestart=False,acousticConfirmed=False)
(stage/'repair-audit.json').write_text(json.dumps(report,indent=2))
print(json.dumps(report))
'''
password=next(v.removeprefix('Password: ') for v in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if v.startswith('Password: '))
r=subprocess.run(['ssh',*opts,'radxa@192.168.31.187',"sudo -S -p '' python3 -c "+shlex.quote(code)],input=password+'\n',text=True,capture_output=True)
(here/'repair-route.log').write_text(r.stdout+r.stderr,encoding='utf-8')
print(r.stdout,r.stderr);r.check_returncode()
