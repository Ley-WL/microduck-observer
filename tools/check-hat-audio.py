"""Bounded ALSA loop test; restores mixer, never touches motor services."""
import array,json,math,subprocess,time,wave
from pathlib import Path
base=Path('/tmp/hat-audio-20261012');base.mkdir(exist_ok=True)
def run(args):
 r=subprocess.run(args,capture_output=True,text=True,timeout=20)
 if r.returncode:raise RuntimeError(str(args)+': '+r.stderr)
 return r.stdout
def mixer(name,value):run(['amixer','-q','-c','1','--','sset',name,value])
run(['alsactl','-f',str(base/'before.state'),'store','1'])
report={}
try:
 for line in run(['amixer','-c','1','scontrols']).splitlines():
  name=line.split("'")[1]
  if 'Bypass' in name and 'Mixer' in name:mixer(name,'off')
  if 'PGA Mixer' in name:mixer(name,'off')
 mixer('Left PGA Mixer Line1L','on');mixer('Right PGA Mixer Line1R','on')
 mixer('PGA','30dB,cap');mixer('PCM','-30dB');mixer('Line','0dB,unmute')
 mixer('Line DAC','0dB');mixer('Right Line Mixer DACR1','on')
 (base/'during.txt').write_text(run(['amixer','-c','1','scontents']))
 rate=48000;frames=array.array('h')
 for n in range(rate*3):
  t=n/rate;active=(0.3<t<0.65 or 1.2<t<1.55 or 2.1<t<2.45)
  v=int(32767*0.1*math.sin(2*math.pi*880*t)) if active else 0
  frames.extend([v,v])
 with wave.open(str(base/'tone.wav'),'wb') as w:w.setparams((2,2,rate,0,'NONE','not compressed'));w.writeframes(frames.tobytes())
 capture=subprocess.Popen(['arecord','-q','-D','hw:1,0','-f','S16_LE','-r','48000','-c','2','-d','12',str(base/'capture.wav')],stderr=subprocess.PIPE)
 time.sleep(3)
 report['playback']=run(['aplay','-q','-D','hw:1,0',str(base/'tone.wav')])
 _,err=capture.communicate(timeout=15)
 if capture.returncode:raise RuntimeError(err.decode())
 with wave.open(str(base/'capture.wav'),'rb') as w:
  samples=array.array('h',w.readframes(w.getnframes()));report['rate']=w.getframerate()
 report['channels']=[]
 for c in range(2):
  x=samples[c::2];mean=sum(x)/len(x);rms=math.sqrt(sum((v-mean)**2 for v in x)/len(x))
  blocks=[]
  for i in range(0,len(x),rate):
   y=x[i:i+rate];mu=sum(y)/len(y);blocks.append(round(math.sqrt(sum((v-mu)**2 for v in y)/len(y)),2))
  report['channels'].append({'channel':c,'rmsAc':rms,'peak':max(abs(v) for v in x),'unique':len(set(x)),'rmsEachSecond':blocks})
 report['result']='capture_and_playback_completed'
finally:
 run(['alsactl','-f',str(base/'before.state'),'restore','1'])
 report['mixerRestored']=True
 (base/'result.json').write_text(json.dumps(report,indent=2))
print(json.dumps(report))
