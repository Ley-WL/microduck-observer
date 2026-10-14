import hashlib,json,subprocess,shutil,time,urllib.request
from pathlib import Path

stage=Path('/home/radxa/audio-repair-20261013')
kernel=subprocess.check_output(['uname','-r'],text=True).strip()
assert kernel=='6.1.115-vendor-rk35xx'
def run(args):return subprocess.check_output(args,text=True,stderr=subprocess.STDOUT)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snap():return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=5))
def idle(s):return s['system']['data']['servoControl']['state'] in ('idle','disabled','failed','holding') and len(s['joints']['data']['servos'])==15 and all(v['online'] and not v['torque'] for v in s['joints']['data']['servos']) and s['joints']['ageMs']<1000
assert idle(snap()),'Active robot; do not restart service'
expected={'snd-soc-tlv320aic3x.ko':'f1c07a1c551196bdf94d732871b604a821b732089e003ec074606316b462b86c',
          'snd-soc-tlv320aic3x-i2c.ko':'2f0241212703d84ea26b6711e511b81d24b09b412d7498a9d2c4b57c0b2c6c6f'}
for name,digest in expected.items():
    assert sha(stage/name)==digest
    assert run(['/usr/sbin/modinfo','-F','vermagic',str(stage/name)]).startswith(kernel+' ')
target=Path('/lib/modules')/kernel/'updates/dkms'
target.mkdir(parents=True,exist_ok=True)
for name,digest in expected.items():
    dest=target/name
    assert not dest.exists() or sha(dest)==digest
    shutil.copy2(stage/name,dest);dest.chmod(0o644)
run(['/usr/sbin/depmod','-a',kernel]);run(['/usr/sbin/modprobe','snd-soc-tlv320aic3x-i2c'])
for _ in range(20):
    if Path('/dev/snd/controlC1').exists():break
    time.sleep(.25)
assert 'aic3104' in Path('/proc/asound/cards').read_text(),'Driver loaded but HAT audio still not registered'
# Persist module loading; retain other module configuration and hardware settings.
Path('/etc/modules-load.d/microduck-hat-audio.conf').write_text('snd_soc_tlv320aic3x_i2c\n')
run(['alsactl','-f',str(stage/'alsa-before.state'),'store','1'])
before_mixer=run(['amixer','-c','1','scontents'])
(stage/'mixer-before.txt').write_text(before_mixer)
controls=run(['amixer','-c','1','scontrols'])
for line in controls.splitlines():
    name=line.split("'")[1]
    if 'Mixer' in name and 'Bypass' in name:run(['amixer','-q','-c','1','--','sset',name,'off'])
for name,value in [('Right Line Mixer DACR1','on'),('PCM','-20dB'),('Line DAC','0dB'),('Line','0dB,unmute')]:
    run(['amixer','-q','-c','1','--','sset',name,value])
run(['alsactl','store','1'])
(stage/'mixer-after.txt').write_text(run(['amixer','-c','1','scontents']))
assert idle(snap())
release=Path('/home/radxa/microduck-observer/current').resolve()
preserved={str(p):sha(p) for directory in [release/'bin',release/'models',Path('/var/lib/ducklink')] for p in directory.rglob('*') if p.is_file()}
for p in Path('/var/lib/microduck-observer').glob('*.json'):preserved[str(p)]=sha(p)
run(['systemctl','restart','microduck-observer'])
for _ in range(60):
    try:
        if idle(snap()):break
    except Exception:pass
    time.sleep(.25)
else:raise RuntimeError('Observer not ready after audio device permission refresh')
assert all(sha(Path(p))==digest for p,digest in preserved.items())
# Same ALSA device/group/cgroup access used by Observer; playback is audio only.
clip=Path('/var/lib/microduck-observer/voice/greet_0.wav')
assert clip.is_file()
play=run(['systemd-run','--quiet','--wait','--pipe','--collect',
          '--unit=microduck-audio230-preview','-p','User=radxa','-p','SupplementaryGroups=audio',
          '-p','DevicePolicy=closed','-p','DeviceAllow=/dev/snd/controlC1 rw','-p','DeviceAllow=/dev/snd/pcmC1D0p rw',
          '/usr/bin/aplay','-q','-D','plughw:CARD=aic3104,DEV=0',str(clip)])
result=dict(repaired=True,kernel=kernel,driverHashes=expected,cards=Path('/proc/asound/cards').read_text(),
            outputBypassDisabled=True,pcmDb=-20,audioPreviewExit=0,audioPreviewSha256=sha(clip),
            acousticConfirmed=False,modelsCalibrationIdentityGainsUnchanged=True,hardwareMotion=False,
            serviceRestart=True,snapshot=snap()['system']['data']['servoControl'])
(stage/'final-audit.json').write_text(json.dumps(result,indent=2))
print(json.dumps(result))
