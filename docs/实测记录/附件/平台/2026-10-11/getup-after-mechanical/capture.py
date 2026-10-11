import json,subprocess
from pathlib import Path
folder=Path(__file__).resolve().parent
host='radxa@8.133.210.70'
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2','-o','HostKeyAlias=192.168.31.193']
ssh=['ssh',*opts,'-p','5722',host]
script='''import json,pathlib,urllib.request
get=lambda p:json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/'+p,timeout=5))
base=pathlib.Path('/var/lib/microduck-observer')
snapshot=get('snapshot')
trace=json.loads((base/'policy-session-latest.json').read_text())
print(json.dumps({'snapshot':snapshot,'policy':get('policy'),'limits':get('servos/limits'),'traceSummary':{'eventTimeMs':trace['eventTimeMs'],'result':trace['result'],'kinds':sorted(set(f.get('kind','') for f in trace['frames']))},'gains':{n:json.loads((base/n).read_text()) for n in ['model-gains.json','walk-model-gains.json']}}))
'''
r=subprocess.run([*ssh,'python3 -'],input=script,text=True,encoding='utf-8',capture_output=True,check=True)
x=json.loads(r.stdout);(folder/'readonly-capture.json').write_text(json.dumps(x,ensure_ascii=False,indent=2),encoding='utf-8')
for name in ['policy-session-latest.json','policy-failure-latest.json']:
 subprocess.run(['scp','-O',*opts,'-P','5722',host+':/var/lib/microduck-observer/'+name,str(folder/name)],check=True)
print(json.dumps({'traceSummary':x['traceSummary'],'control':x['snapshot']['system']['data']['servoControl'],'gains':x['gains'],'rows':[{k:r.get(k) for k in ['id','torque','kpRaw','kiRaw','kdRaw','accelerationRaw','speedLimitRaw','torqueLimitRaw','position','load']} for r in x['snapshot']['joints']['data']['servos']]},ensure_ascii=False,indent=2))
