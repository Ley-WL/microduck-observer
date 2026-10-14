
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
