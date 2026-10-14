from pathlib import Path
import subprocess,shlex

here=Path(__file__).resolve().parent
root=here.parents[5]
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','UserKnownHostsFile='+str(root/'deploy/staging/new-board-known-hosts')]
code='''import subprocess,json,pathlib
commands=[['dmesg'],['ls','/sys/bus/i2c/devices'],['find','/lib/modules','-iname','*aic3*'],['cat','/proc/asound/cards'],['systemctl','show','microduck-observer','-p','SupplementaryGroups','-p','DeviceAllow']]
results={}
for command in commands:
    output=subprocess.run(command,capture_output=True,text=True).stdout
    if command==['dmesg']:output='\\n'.join(line for line in output.splitlines() if any(x in line.lower() for x in ['aic3','tlv320','asoc','i2s','snd','codec','overlay','pinctrl','fe5a','fe5c']))
    results[' '.join(command)]=output
print(json.dumps(results,indent=2))
'''
password=next(v.removeprefix('Password: ') for v in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if v.startswith('Password: '))
result=subprocess.run(['ssh',*opts,'radxa@192.168.31.230',"sudo -S -p '' python3 -c "+shlex.quote(code)],input=password+'\n',text=True,capture_output=True)
(here/'after-boot-diagnostics.json').write_text(result.stdout,encoding='utf-8')
print(result.stdout,result.stderr);result.check_returncode()
