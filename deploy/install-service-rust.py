"""One-shot installer only. Both installed runtime helpers are Rust executables."""
from pathlib import Path
import hashlib, json, os, shutil, socket, subprocess, time, urllib.request

stage=Path('/home/radxa/service-rust-stage')
release=Path('/usr/local/lib/microduck-services/20261011-service-rust-v1')
backup=Path('/var/lib/microduck-observer/service-rust-backup-20261011')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def run(*args):return subprocess.run(args,check=True,capture_output=True,text=True,timeout=15).stdout.strip()
def snapshot():return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/snapshot',timeout=4))
def unix(path,body):
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as s:
        s.settimeout(5);s.connect(path);s.sendall(json.dumps(body).encode()+b'\n');s.shutdown(socket.SHUT_WR)
        raw=b''
        while True:
            b=s.recv(4096)
            if not b:break
            raw+=b
        return json.loads(raw)
def physical(cal):
    cal=json.loads(json.dumps(cal));cal.pop('revision',None);cal.pop('updatedAt',None);return cal
before=snapshot()
assert before['system']['data']['servoControl']['state'] in ('idle','disabled','failed'),'Stop control before deployment'
assert all(r['torque']==0 for r in before['joints']['data']['servos']),'Enabled servos; no replacement'
assert not release.exists() and not backup.exists(),'Already staged/deployed; inspect instead of rerun'
hashes=json.loads((stage/'hashes.json').read_text())
for name,digest in hashes.items():assert sha(stage/name)==digest
netplan=Path('/etc/netplan/30-microduck-wifi.yaml');net_hash=sha(netplan)
old_wifi=unix('/run/ducklink-wifi/control.sock',{'op':'status'})
assert 'error' not in old_wifi and old_wifi['job']['state']!='running'
current=Path('/home/radxa/microduck-observer/current').resolve()
unchanged={str(p):sha(p) for d in [current/'models',current/'bin',Path('/var/lib/ducklink')] for p in d.rglob('*') if p.is_file()}
for p in [Path('/var/lib/microduck-observer/model-gains.json')]:
    if p.exists():unchanged[str(p)]=sha(p)
backup.mkdir(mode=0o700,parents=True);release.mkdir(mode=0o755,parents=True)
paths=[Path('/etc/systemd/system/ducklink-wifi.service.d/runtime-rust.conf'),Path('/etc/systemd/system/microduck-power@.service')]
original={str(p):p.read_bytes() if p.exists() else None for p in paths}
for i,p in enumerate(paths):
    if p.exists():shutil.copy2(p,backup/f'unit-{i}')
for name in hashes:
    shutil.copy2(stage/name,release/name);(release/name).chmod(0o755)
wifi_override=f'[Service]\nExecStart=\nExecStart={release}/ducklink-wifi\n'
power_unit=f'''[Unit]
Description=MicroDuck restricted Rust power request
[Service]
ExecStart={release}/microduck-power-helper
StandardInput=socket
StandardOutput=socket
StandardError=journal
TimeoutStartSec=4
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
'''
try:
    # Recheck immediately before replacement. Wi-Fi helper owns no motor UART.
    now=snapshot();assert now['system']['data']['servoControl']['state'] in ('idle','disabled','failed') and all(r['torque']==0 for r in now['joints']['data']['servos'])
    paths[0].parent.mkdir(parents=True,exist_ok=True);paths[0].write_text(wifi_override);paths[1].write_text(power_unit)
    run('systemctl','daemon-reload');run('systemctl','restart','ducklink-wifi')
    time.sleep(2)
    run('systemctl','is-active','ducklink-wifi');run('systemctl','is-active','microduck-observer')
    assert '/usr/local/lib/microduck-services/' in run('systemctl','show','ducklink-wifi','-p','ExecStart','--value')
    status=unix('/run/ducklink-wifi/control.sock',{'op':'status'});assert 'error' not in status
    assert status['current']==old_wifi['current'],'Wi-Fi association changed'
    assert sha(netplan)==net_hash
    assert 'error' in unix('/run/ducklink-wifi/control.sock',{'op':'invalid'})
    assert not unix('/run/microduck-power.sock',{'action':'invalid'})['accepted']
    # A fixed invalid request cannot create a power timer.
    assert subprocess.run(['systemctl','is-active','microduck-power-pending.timer'],capture_output=True,text=True).stdout.strip()!='active'
    for file,digest in unchanged.items():assert sha(Path(file))==digest
    assert physical(snapshot()['calibration'])==physical(before['calibration'])
    scans=[]
    for _ in range(20):
        s=snapshot();rows=s['joints']['data']['servos'];assert all(r['online'] and r['torque']==0 for r in rows)
        assert not s['joints']['data']['diagnostics']['missingIds'];scans.append({'state':s['system']['data']['servoControl']['state'],'missing':[]});time.sleep(.1)
    assert all(subprocess.run(['systemctl','is-enabled',svc],capture_output=True,text=True).stdout.strip()=='masked' for svc in ['tofd','rsyslog'])
    # Archive old runtime sources for rollback, remove active Python helper paths.
    for i,p in enumerate([Path('/opt/ducklink-gateway/wifi_service.py'),Path('/usr/local/lib/microduck-power-helper.py')]):
        if p.exists():shutil.copy2(p,backup/f'legacy-runtime-{i}.py');p.unlink()
    procs=run('ps','-eo','pid,comm,args')
    assert not any('wifi_service.py' in line or 'microduck-power-helper.py' in line for line in procs.splitlines())
    evidence={'installed':True,'helperRelease':str(release),'observerRelease':str(current),'binaryHashes':hashes,'netplanUnchanged':True,'currentWifi':{'connected':status['current']['connected'],'state':status['current']['state']},'preservedFiles':unchanged,'calibrationUnchanged':True,'allDisabledSnapshots':scans,'hardwareMotion':False,'networkSwitch':False,'powerAction':False,'rollbackDirectory':str(backup),'runtimeProcesses':[line for line in procs.splitlines() if 'ducklink-wifi' in line or 'microduck-observer/current/bin/' in line]}
    (stage/'deployment.json').write_text(json.dumps(evidence,ensure_ascii=False,indent=2));print(json.dumps({k:v for k,v in evidence.items() if k not in ['preservedFiles','allDisabledSnapshots']},ensure_ascii=False))
except Exception:
    for p,raw in original.items():
        path=Path(p)
        if raw is None:path.unlink(missing_ok=True)
        else:path.write_bytes(raw)
    for i,p in enumerate([Path('/opt/ducklink-gateway/wifi_service.py'),Path('/usr/local/lib/microduck-power-helper.py')]):
        if (backup/f'legacy-runtime-{i}.py').exists():shutil.copy2(backup/f'legacy-runtime-{i}.py',p)
    run('systemctl','daemon-reload');run('systemctl','restart','ducklink-wifi');raise
