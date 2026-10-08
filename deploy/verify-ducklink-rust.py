"""Read-only unified-service verification; never writes a motion/BlueZ control command."""
import hashlib,json,re,subprocess,urllib.request
from pathlib import Path
import dbus

def get(path):return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/'+path,timeout=3))
def run(*parts):return subprocess.check_output(parts,text=True).strip()
pid=run('systemctl','show','microduck-observer','-p','MainPID','--value')
rows=run('busctl','--system','list','--no-legend').splitlines()
name=next(row.split()[0] for row in rows if row.split()[1]==pid and row.startswith(':'))
# BlueZ advertising count is public; local GATT introspection is restricted by D-Bus policy.
advertising=run('busctl','--system','get-property','org.bluez','/org/bluez/hci0','org.bluez.LEAdvertisingManager1','ActiveInstances')
assert int(advertising.split()[-1])>=1
gatt=[];ad=[{'activeInstances':int(advertising.split()[-1])}]
code="import socket,json; s=socket.socket(socket.AF_UNIX); s.settimeout(3); s.connect('/run/ducklink-wifi/control.sock'); s.sendall(b'{\"type\":\"wifi\",\"op\":\"status\"}\\n'); d=s.recv(8000); v=json.loads(d); print(json.dumps({'responded':True,'error':v.get('error'),'connected':v.get('current',{}).get('connected')}))"
wifi=json.loads(run('runuser','-u','radxa','-G','ducklink','--','/usr/bin/python3','-c',code));assert wifi.get('error') is None
snap=get('snapshot');ctrl=snap['system']['data']['servoControl'];assert ctrl['state'] not in ('policy','moving','preflight','disabling')
manifest={'result':'verified','health':get('health'),'policyModels':get('policy'),'control':ctrl,'gatt':gatt,'gattIntrospection':'unavailable by local D-Bus policy; registration confirmed by health ready','advertisements':ad,'wifiReadonly':wifi,'unifiedService':run('systemctl','is-active','microduck-observer'),'legacyState':subprocess.run(['systemctl','is-active','ducklink-ble'],capture_output=True,text=True).stdout.strip(),'legacyEnabled':subprocess.run(['systemctl','is-enabled','ducklink-ble'],capture_output=True,text=True).stdout.strip(),'identitySha256':hashlib.sha256(Path('/var/lib/ducklink/identity.json').read_bytes()).hexdigest()}
assert manifest['health']['bluetooth']['state']=='ready' and manifest['legacyState']=='inactive' and manifest['legacyEnabled']=='disabled'
Path('/home/radxa/ducklink-rust-verification.json').write_text(json.dumps(manifest))
print(json.dumps({'result':'verified','bluetooth':manifest['health']['bluetooth'],'gattCount':len(gatt),'advertising':True,'wifiReadonly':wifi,'legacyState':manifest['legacyState'],'controlState':ctrl['state']}))
