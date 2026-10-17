"""Scoped binary update: user confirmed servos disconnected/power removed on board87."""
import hashlib, json, os, shutil, subprocess, time, urllib.request
from pathlib import Path

root = Path('/home/radxa/microduck-observer')
stage = Path('/tmp/gamepad-auto-20261017')
stage.mkdir(exist_ok=True)
candidate = Path('/tmp/microduck-gamepad-auto-20261017')
expected = '0a2c1bbf8a5921e41caf5462143cc6735655364ccec01561aef188174d9d97ef'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def get(path): return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/' + path, timeout=5))
def idle(s):
    assert s['system']['data']['servoControl']['state'] in ['idle', 'disabled', 'failed'], 'Active motion'
    assert not any(r.get('torque') for r in s['joints']['data']['servos']), 'Reported enabled servo'
def run(*args, **kwargs): return subprocess.run(args, check=True, capture_output=True, text=True, **kwargs).stdout
def point(p):
    tmp = root / 'current.gamepad-staging'
    assert not tmp.exists() and not tmp.is_symlink()
    tmp.symlink_to(p)
    os.replace(tmp, root / 'current')

assert run('hostname').strip() == 'microduck-1c15e7'
assert sha(candidate) == expected
before = get('snapshot'); idle(before)
mode = get('gamepad/mode'); gains = get('model-gains')
old = (root / 'current').resolve()
new = root / 'releases' / 'runtime-20261017-gamepad-auto-v2'
assert not new.exists()
preserved = {str(p): sha(p) for base in [Path('/var/lib/ducklink'), Path('/var/lib/microduck-observer')]
             for p in base.glob('*.json') if p.is_file()}
assets = {str(p.relative_to(old)): sha(p) for p in old.rglob('*')
          if p.is_file() and p.relative_to(old).as_posix() != 'bin/microduck-observer'}
shutil.copytree(old, new, symlinks=True)
dest = new / 'bin/microduck-observer'
assert not dest.is_symlink()
shutil.copy2(candidate, dest); dest.chmod(0o755)
assert sha(dest) == expected
env = dict(os.environ, MICRODUCK_POLICY_PATH=str(new/'models/xgoduck_walk.onnx'),
           ORT_DYLIB_PATH=str(new/'lib/libonnxruntime.so'),
           MICRODUCK_GAINS_FILE='/var/lib/microduck-observer/model-gains.json')
(stage/'model-bank.json').write_text(run(str(dest), '--verify-model-bank', env=env))
# Controller may be disconnected: daemon must start and wait without it.
check=subprocess.run(['runuser','-u','radxa','-g','radxa','-G','input','--',str(dest),'--verify-gamepad'],capture_output=True,text=True)
(stage/'input-check.json').write_text(json.dumps({'returncode':check.returncode,'stdout':check.stdout,'stderr':check.stderr}))
(stage/'before.json').write_text(json.dumps(before))
idle(get('snapshot'))
try:
    run('systemctl', 'stop', 'microduck-observer')
    point(new)
    run('systemctl', 'start', 'microduck-observer')
    for _ in range(60):
        try:
            after = get('snapshot'); idle(after)
            if after['gamepad']['bootId'] != before['gamepad']['bootId']: break
        except Exception: pass
        time.sleep(.25)
    else: raise RuntimeError('New service snapshot unavailable')
    assert all(sha(Path(p)) == h for p,h in preserved.items()), 'Persistent settings changed'
    assert all(sha(new/p) == h for p,h in assets.items()), 'Release assets changed'
    assert get('gamepad/mode') == mode and get('model-gains') == gains
    samples=[]
    for _ in range(20):
        s=get('snapshot');idle(s)
        assert s['gamepad']['bootId'] == after['gamepad']['bootId']
        samples.append({'seq':s['gamepad']['seq'],'ageMs':s['gamepad']['ageMs']})
        time.sleep(.1)
    if after['gamepad']['data']['connected']: assert samples[-1]['seq'] > samples[0]['seq']
    for p in (new/'frontend/dist').rglob('*'):
        if p.is_file():
            content=urllib.request.urlopen('http://127.0.0.1:8877/'+p.relative_to(new/'frontend/dist').as_posix(),timeout=5).read()
            assert hashlib.sha256(content).hexdigest()==sha(p)
    run('systemctl','is-active','microduck-observer')
    report={'installed':True,'release':str(new),'previousRelease':str(old),'binarySha256':expected,
            'settingsModelsFrontendUnchanged':True,'gamepad':after['gamepad'],'samples':samples,
            'gamepadMode':mode,'frontendHttpHashesVerified':True,'hardwareMotion':False,
            'hardwareBenchmark':'not run: user confirmed servos disconnected or power removed; feedback unavailable'}
    (stage/'after.json').write_text(json.dumps(after))
    (stage/'deployment.json').write_text(json.dumps(report,indent=2))
    print(json.dumps(report))
except BaseException:
    subprocess.run(['systemctl','stop','microduck-observer'])
    point(old)
    subprocess.run(['systemctl','start','microduck-observer'])
    raise
