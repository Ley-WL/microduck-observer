from pathlib import Path
import subprocess

password = next(s.removeprefix('Password: ') for s in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if s.startswith('Password: '))
ssh = ['ssh', '-p', '22', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=5', '-o', 'HostKeyAlias=192.168.31.193', 'radxa@192.168.31.187']
remote = r'''
import subprocess,json,hashlib
from pathlib import Path
def run(*args):
    return subprocess.run(args,check=True,capture_output=True,text=True).stdout
before=run('amixer','-c','aic3104','scontents')
run('alsactl','-f','/tmp/microduck-quack-before.state','store','1')
try:
    for section in before.split('Simple mixer control'):
        if 'Bypass' in section.split('\n')[0] and 'switch' in section:
            name=section.split("'",2)[1]
            run('amixer','-c','aic3104','--','sset',name,'off')
    run('amixer','-c','aic3104','--','sset','PCM','-10dB')
    run('amixer','-c','aic3104','--','sset','Line','0dB','unmute')
    playback=run('aplay','-D','plughw:CARD=aic3104,DEV=0','/tmp/microduck-quack-48k-stereo.wav')
finally:
    run('alsactl','-f','/tmp/microduck-quack-before.state','restore','1')
after=run('amixer','-c','aic3104','scontents')
assert before==after, 'mixer restoration mismatch'
print(json.dumps({'playbackExit':0,'card':'aic3104','interface':'J1','format':'48000Hz stereo quack same as prior silent test','analogBypassDisabled':True,'temporaryPcmDb':-10,'temporaryLineDb':0,'mixerRestored':True,'servoCommands':False,'audibleAcceptancePending':True}))
'''
script = Path(__file__).with_name('preview-quack-remote.py')
script.write_text(remote,encoding='utf-8')
subprocess.run(['scp','-P','22','-o','BatchMode=yes','-o','ConnectTimeout=5','-o','HostKeyAlias=192.168.31.193',str(script),'radxa@192.168.31.187:/tmp/microduck-quack-preview.py'],check=True)
command = "sudo -S -p '' python3 /tmp/microduck-quack-preview.py"
result = subprocess.run([*ssh, command], input=password+'\n', text=True, encoding='utf-8', capture_output=True)
print(result.stdout)
print(result.stderr)
if result.returncode==0:
    Path(__file__).with_name('quack-no-bypass-result.json').write_text(result.stdout,encoding='utf-8')
raise SystemExit(result.returncode)
