from pathlib import Path
import subprocess
base=Path(__file__).resolve().parent
password=next(s.removeprefix('Password: ') for s in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if s.startswith('Password: '))
options=['-o','BatchMode=yes','-o','ConnectTimeout=5','-o','HostKeyAlias=192.168.31.193']
remote=r'''
import subprocess,json
from pathlib import Path
def run(*args):return subprocess.run(args,check=True,capture_output=True,text=True).stdout
before=run('amixer','-c','aic3104','scontents')
run('alsactl','-f','/tmp/microduck-j1-repair-before.state','store','1')
try:
    disabled=[]
    for section in before.split('Simple mixer control'):
        if 'Bypass' in section.split('\n')[0] and 'switch' in section:
            name=section.split("'",2)[1]
            run('amixer','-c','aic3104','--','sset',name,'off');disabled.append(name)
    run('amixer','-c','aic3104','--','sset','PCM','-10dB')
    run('amixer','-c','aic3104','--','sset','Line','0dB','unmute')
    run('amixer','-c','aic3104','--','sset','Right Line Mixer DACR1','on')
    after=run('amixer','-c','aic3104','scontents')
    # Capture routing and gains must remain identical.
    def capture_sections(text):
        return [s for s in text.split('Simple mixer control') if any(x in s.split('\n')[0] for x in ['PGA Mixer','PGA\'','AGC\'','ADC HPF','Mic Bias'])]
    assert capture_sections(before)==capture_sections(after)
    run('alsactl','store','1')
    run('aplay','-D','plughw:CARD=aic3104,DEV=0','/tmp/microduck-quack-preview-20261012.wav')
    print(json.dumps({'outputBypassDisabled':disabled,'pcmDb':-10,'lineDb':0,'captureSettingsUnchanged':True,'alsaSettingsStored':True,'originalQuackPlaybackExit':0,'servoCommands':False,'physicalOriginalQuackAcceptancePending':True}))
except:
    run('alsactl','-f','/tmp/microduck-j1-repair-before.state','restore','1');raise
'''
p=base/'configure-j1-remote.py';p.write_text(remote,encoding='utf-8')
subprocess.run(['scp','-P','22',*options,str(p),'radxa@192.168.31.187:/tmp/microduck-configure-j1.py'],check=True)
r=subprocess.run(['ssh','-p','22',*options,'radxa@192.168.31.187',"sudo -S -p '' python3 /tmp/microduck-configure-j1.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(r.stdout+r.stderr)
if r.returncode==0:(base/'j1-output-repair.json').write_text(r.stdout,encoding='utf-8')
raise SystemExit(r.returncode)
