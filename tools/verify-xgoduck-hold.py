"""Read deployed files and reject inactive drive requests; never enables motors."""
import hashlib,json,re,urllib.request,urllib.error
from pathlib import Path
root=Path(__file__).resolve().parents[1]
base='http://192.168.31.186:8877'
def get(route):
    return urllib.request.urlopen(base+route,timeout=5).read()
def snapshot(): return json.loads(get('/api/v1/snapshot'))
before=snapshot()
assert before['system']['data']['servoControl']['state'] not in ('policy','preflight','moving','disabling')
html=get('/')
assert html==(root/'frontend/dist/index.html').read_bytes()
assets={}
for asset in re.findall(r'(?:src|href)="(/assets/[^\"]+)"',html.decode()):
    remote=get(asset);assert remote==(root/'frontend/dist'/asset.lstrip('/')).read_bytes()
    assets[asset]=hashlib.sha256(remote).hexdigest()
tests=[]
for twist,code in [([.2,0,0],409),([.3,0,0],400),([0,0,0],409)]:
    request=urllib.request.Request(base+'/api/v1/policy/command',data=json.dumps({'session':'inactive','sequence':1,'twist':twist}).encode(),headers={'Content-Type':'application/json'},method='POST')
    try: urllib.request.urlopen(request,timeout=5);raise AssertionError('Inactive drive accepted')
    except urllib.error.HTTPError as error:
        assert error.code==code
        tests.append({'twist':twist,'status':error.code,'body':json.loads(error.read())})
after=snapshot()
assert after['system']['data']['servoControl']==before['system']['data']['servoControl']
report={'assets':assets,'rejectedCommands':tests,'stateBefore':before['system']['data']['servoControl'],'stateAfter':after['system']['data']['servoControl'],'hardwareMotionTest':False}
dest=root/'docs/实测记录/附件/平台/2026-10-07/xgoduck-hold/frontend-api-check.json'
dest.write_text(json.dumps(report,indent=2,ensure_ascii=False),encoding='utf-8')
print(json.dumps({'staticAssetsMatch':True,'rejectedCommands':len(tests),'controlUnchanged':True}))
