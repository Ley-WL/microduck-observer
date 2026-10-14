from pathlib import Path
import subprocess,shlex

here=Path(__file__).resolve().parent
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','HostKeyAlias=192.168.31.193']
code='''import pathlib,shutil,subprocess,json,hashlib
script=pathlib.Path('/usr/local/bin/aic3104-init.sh')
stage=pathlib.Path('/home/radxa/audio-repair-187-20261013')
text=script.read_text()
marker='        # MicroDuck: speaker output uses DAC, never analog input bypass.'
assert marker not in text,'Already patched; do not overwrite backup'
anchor='    if amixer -c aic3104 info >/dev/null 2>&1; then\\n'
assert text.count(anchor)==1
names=['Left Line Mixer Line2L Bypass','Left Line Mixer Line2R Bypass','Left Line Mixer PGAL Bypass','Left Line Mixer PGAR Bypass','Right Line Mixer Line2L Bypass','Right Line Mixer Line2R Bypass','Right Line Mixer PGAL Bypass','Right Line Mixer PGAR Bypass']
block=marker+'\\n'+''.join("        amixer -c aic3104 sset '"+name+"' off >/dev/null\\n" for name in names)
block+="        amixer -c aic3104 sset 'Right Line Mixer DACR1' on >/dev/null\\n"
backup=stage/'aic3104-init.before.sh';assert not backup.exists();shutil.copy2(script,backup)
script.write_text(text.replace(anchor,anchor+block));script.chmod(0o755)
subprocess.run(['bash','-n',str(script)],check=True)
subprocess.run(['alsactl','store','1'],check=True)
report=dict(userHeardPreview=True,bootInitBypassOffAdded=True,syntaxChecked=True,
            sourceSha256=hashlib.sha256(script.read_bytes()).hexdigest(),
            previousSourceSha256=hashlib.sha256(backup.read_bytes()).hexdigest(),
            otherBootInitCommandsUnchanged=True,serviceRestart=False,rebootValidation=False,hardwareMotion=False)
(stage/'persistent-audit.json').write_text(json.dumps(report,indent=2))
print(json.dumps(report))
'''
password=next(v.removeprefix('Password: ') for v in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if v.startswith('Password: '))
r=subprocess.run(['ssh',*opts,'radxa@192.168.31.187',"sudo -S -p '' python3 -c "+shlex.quote(code)],input=password+'\n',text=True,capture_output=True)
(here/'persist-route.log').write_text(r.stdout+r.stderr,encoding='utf-8')
print(r.stdout,r.stderr);r.check_returncode()
