import json,subprocess,hashlib
from pathlib import Path

stage=Path('/home/radxa/audio-repair-20261013')
def run(args):return subprocess.check_output(args,text=True,stderr=subprocess.STDOUT)
before=run(['amixer','-c','1','sget','PCM'])
run(['amixer','-q','-c','1','--','sset','PCM','-10dB'])
run(['alsactl','store','1'])
after=run(['amixer','-c','1','sget','PCM'])
clip=Path('/var/lib/microduck-observer/voice/greet_1.wav')
run(['runuser','-u','radxa','--','aplay','-q','-D','plughw:CARD=aic3104,DEV=0',str(clip)])
result=dict(pcmDb=-10,previousPcmDb=-20,persistent=True,audioPreviewExit=0,
            initialSoundConfirmedByUser=True,userRequestedLouder=True,
            louderSoundConfirmedByUser=False,hardwareMotion=False,before=before,after=after,
            audioSha256=hashlib.sha256(clip.read_bytes()).hexdigest())
(stage/'volume-audit.json').write_text(json.dumps(result,indent=2))
print(json.dumps(result))
