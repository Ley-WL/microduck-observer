"""Overlay Rust binary and Web assets on the current board release; retain models/settings."""
import argparse, hashlib, json, shlex, subprocess, tarfile
from datetime import datetime, timezone, timedelta
from pathlib import Path

root=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--host',default='radxa@192.168.31.186')
parser.add_argument('--port',type=int,default=22)
parser.add_argument('--host-key-alias',help='Verified SSH host identity; defaults to the host name')
parser.add_argument('--known-hosts',type=Path,help='Known-hosts file containing the verified target host key')
parser.add_argument('--package-only',action='store_true')
parser.add_argument('--binary-only',action='store_true',help='Retain the current board frontend without overlaying local assets')
parser.add_argument('--preserve-device-settings',action='store_true',help='Skip audio/input preparation and retain existing service settings')
parser.add_argument('--align-walk-scale',action='store_true',help='Apply only the verified walk metadata action scale 0.9 -> 1.0 update')
parser.add_argument('--flamingo-model',action='store_true',help='Add flamingo actor and verify hold/lower ARM fixtures')
parser.add_argument('--rear-hook-model',action='store_true',help='Add v27 rear-hook actor, preserve legacy models/settings, and verify native fixtures')
parser.add_argument('--rear-hook-native-model',action='store_true',help='Add encoder-physics rear-hook candidate and verify board calibration plus ARM fixtures')
parser.add_argument('--rear-hook-gated-model',action='store_true',help='Add settle-before-lift rear-hook candidate with an independent contract')
parser.add_argument('--dance-model',action='store_true',help='Add the dance10 model and verify ARM inference before activation')
parser.add_argument('--retire-legacy-models',action='store_true',help='Remove retired stand/walk files from the new release and verify common gains')
args=parser.parse_args()
assert sum([args.rear_hook_model,args.rear_hook_native_model,args.rear_hook_gated_model])<=1,'Select one rear-hook contract'
name='runtime-'+datetime.now(timezone(timedelta(hours=8))).strftime('%Y%m%d-%H%M%S')
stage=root/'deploy/staging'/name;stage.mkdir(parents=True)
evidence=root/'docs/实测记录/附件/平台'/datetime.now(timezone(timedelta(hours=8))).strftime('%Y-%m-%d')/name
evidence.mkdir(parents=True)
binary=root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer'
assert binary.read_bytes()[:4]==b'\x7fELF' and int.from_bytes(binary.read_bytes()[18:20],'little')==183
assert b'/api/v1/tof' in binary.read_bytes(), 'Build the current Observer source: ToF API missing from ARM binary'
if args.rear_hook_native_model or args.rear_hook_gated_model:
    marker=b'RearHook-OneShot-3s-NativeEncoderGated' if args.rear_hook_gated_model else b'RearHook-OneShot-3s-NativeEncoder'
    assert marker in binary.read_bytes(),'Finish building native rear-hook runtime before packaging'
with tarfile.open(stage/'payload.tar.gz','w:gz') as archive:
    archive.add(binary,arcname='bin/microduck-observer')
    if args.preserve_device_settings:
        marker=stage/'preserve-device-settings.json';marker.write_text('{}')
        archive.add(marker,arcname=marker.name)
    if args.align_walk_scale:
        archive.add(root/'models/xgoduck_walk.metadata.json',arcname='walk-scale-update.json')
    if args.retire_legacy_models:
        marker=stage/'common-gains-migration.json';marker.write_text('{}')
        archive.add(marker,arcname=marker.name)
    if args.flamingo_model:
        for suffix in ['onnx','metadata.json']:
            archive.add(root/('models/flamingo.'+suffix),arcname='models/flamingo.'+suffix)
        for mode in ['hold','lower']:
            archive.add(root/f'backend-rust/tests/fixtures/policy-flamingo_{mode}.json',arcname=f'policy-flamingo_{mode}.json')
    if args.rear_hook_model:
        for suffix in ['onnx','metadata.json']:
            archive.add(root/('models/rear_hook_v27.'+suffix),arcname='models/rear_hook_v27.'+suffix)
        for tick in range(4):
            archive.add(root/f'backend-rust/tests/fixtures/policy-rear-{tick}.json',arcname=f'policy-rear-{tick}.json')
    if args.rear_hook_native_model or args.rear_hook_gated_model:
        stem='rear_hook_native_gated' if args.rear_hook_gated_model else 'rear_hook_native'
        for suffix in ['onnx','metadata.json']:
            archive.add(root/('models/'+stem+'.'+suffix),arcname='models/'+stem+'.'+suffix)
        for tick in range(4):
            archive.add(root/f'backend-rust/tests/fixtures/policy-rear-native-{tick}.json',arcname=f'policy-rear-native-{tick}.json')
    if args.dance_model:
        for suffix in ['onnx','metadata.json']:
            archive.add(root/('models/dance10.'+suffix),arcname='models/dance10.'+suffix)
        for tick in [0,80,200,350]:
            archive.add(root/f'backend-rust/tests/fixtures/policy-dance-{tick}.json',arcname=f'policy-dance-{tick}.json')
    archive.add(binary.with_name('microduck-tof'),arcname='bin/microduck-tof')
    for notice in ['LICENSE.txt','APACHE-2.0.txt','UPSTREAM.md']:
        archive.add(root/'backend-rust/vendor/tof'/notice,arcname='licenses/tof/'+notice)
    for notice in ['LICENSE','UPSTREAM.md']:
        archive.add(root/'backend-rust/vendor/microduck-sounds'/notice,arcname='licenses/microduck-sounds/'+notice)
    if not args.binary_only:
        archive.add(root/'frontend/dist',arcname='frontend/dist')
    archive.add(root/'deploy/imu-installation.default.json',arcname='imu-installation.default.json')
    archive.add(root/'deploy/audio_prepare.py',arcname='hardware/audio/audio_prepare.py')
    archive.add(root/'deploy/hardware/audio',arcname='hardware/audio')
    for tool in ['verify_rust_hardware.py','measure_runtime.py','ws_reader.py']:
        archive.add(root/'deploy'/tool,arcname=tool)
installer=r'''
import hashlib,json,os,shutil,subprocess,sys,tarfile,time,urllib.request,urllib.error
from pathlib import Path
stage=Path(sys.argv[1]);name=sys.argv[2];root=Path('/home/radxa/microduck-observer')
assert name.startswith('runtime-') and name.replace('-','').isalnum()
def get(path):return json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/'+path,timeout=5))
def idle(s):return s['system']['data']['servoControl']['state'] not in ['preflight','moving','policy','stopping','disabling'] and len(s['joints']['data']['servos'])==15 and all(r['online'] and not r['torque'] for r in s['joints']['data']['servos'])
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
before=get('snapshot');assert idle(before),'Active robot; leave production unchanged'
try:
    gamepad_mode=get('gamepad/mode')['debug']
except urllib.error.HTTPError as error:
    if error.code!=404:raise
    gamepad_mode=None  # Legacy runtime has no mode endpoint; retain configured startup defaults.
(stage/'gains-before.json').write_text(json.dumps(get('model-gains')))
old=(root/'current').resolve();new=root/'releases'/name;assert not new.exists()
preserved={str(p):sha(p) for base in [old/'models',Path('/var/lib/ducklink')] for p in base.rglob('*') if p.is_file()}
for p in Path('/var/lib/microduck-observer').glob('*.json'):preserved[str(p)]=sha(p)
(stage/'preserved-sha256.json').write_text(json.dumps(preserved,indent=2))
(stage/'previous-release.txt').write_text(str(old))
service_states={svc:subprocess.run(['systemctl','is-enabled',svc],capture_output=True,text=True).stdout.strip() for svc in ['tofd','rsyslog']}
shutil.copytree(old,new,symlinks=True)
with tarfile.open(stage/'payload.tar.gz') as archive:
    assert all((new/m.name).resolve().is_relative_to(new.resolve()) and (m.isfile() or m.isdir()) for m in archive.getmembers())
    archive.extractall(new)
os.chmod(new/'bin/microduck-observer',0o755)
os.chmod(new/'bin/microduck-tof',0o755)
preserve_device_settings=(new/'preserve-device-settings.json').exists()
if preserve_device_settings:(new/'preserve-device-settings.json').unlink()
assert all(sha(Path(p))==v for p,v in preserved.items())
walk_update=None
marker=new/'walk-scale-update.json'
if marker.exists():
    filename='xgoduck_walk.metadata.json'
    prior_meta=json.loads((old/'models'/filename).read_text())
    expected=json.loads(json.dumps(prior_meta))
    assert expected['policySha256']=='1cbf8730eadd75a5704332c9f3c10f60bfa9412135b907831c9dbd731ce1795f'
    assert expected['walkExecution']['actionScale']==0.9
    expected['walkExecution']['actionScale']=1.0
    expected['actionSemantics']='HOME + 1.0 * raw policy action; head/leg target filter new weights 0.5/0.7; first target passthrough'
    assert json.loads(marker.read_text())==expected,'Unexpected walk metadata change'
    assert sha(new/'models/xgoduck_walk.onnx')==expected['policySha256']
    assert (new/'models'/filename).resolve().is_relative_to(new.resolve())
    shutil.copy2(marker,new/'models'/filename);marker.unlink()
    walk_update={'file':filename,'before':0.9,'after':1.0,'metadataSha256':sha(new/'models'/filename)}
retired=[];bank_reports=[]
if (new/'common-gains-migration.json').exists():
    for stem in ['hd1910-head-v5','hd1910-walk-v6-symmetry500']:
        for suffix in ['onnx','metadata.json']:
            p=new/'models'/f'{stem}.{suffix}'
            assert p.resolve().is_relative_to(new.resolve())
            if p.exists():p.unlink();retired.append(p.name)
    (new/'common-gains-migration.json').unlink()
    env=dict(os.environ,MICRODUCK_POLICY_PATH=str(new/'models/xgoduck_walk.onnx'),ORT_DYLIB_PATH=str(new/'lib/libonnxruntime.so'))
    for gains_file in [Path('/var/lib/microduck-observer/model-gains.json'),stage/'test-common-gains.json']:
        if gains_file.parent==stage:
            gains_file.write_text('{"p":7,"d":9}')
            (stage/'walk-model-gains.json').write_text('{"p":1,"d":2}')
        expected=json.loads(gains_file.read_text());env['MICRODUCK_GAINS_FILE']=str(gains_file)
        result=subprocess.run([str(new/'bin/microduck-observer'),'--verify-model-bank'],env=env,capture_output=True,text=True,check=True)
        report=json.loads(result.stdout.splitlines()[-1]);assert len(report['models'])==9
        assert all(m['p']==expected['p'] and m['d']==expected['d'] for m in report['models'])
        bank_reports.append(report)
    (stage/'common-gains-arm.json').write_text(json.dumps(bank_reports,indent=2))
for p in (old/'models').rglob('*'):
    if p.is_file() and p.name not in retired and not (walk_update and p.name==walk_update['file']):assert sha(new/'models'/p.relative_to(old/'models'))==sha(p),'Existing model changed'
fixture_reports=[]
if (new/'policy-dance-0.json').exists():
    env=dict(os.environ,MICRODUCK_POLICY_PATH=str(new/'models/xgoduck_walk.onnx'),ORT_DYLIB_PATH=str(new/'lib/libonnxruntime.so'))
    for tick in [0,80,200,350]:
        fixture=new/f'policy-dance-{tick}.json'
        result=subprocess.run([str(new/'bin/microduck-observer'),'--verify-policy-fixture',str(fixture)],env=env,capture_output=True,text=True,check=True)
        fixture_reports.append(json.loads(result.stdout.splitlines()[-1]));fixture.unlink()
    (stage/'dance-arm-fixtures.json').write_text(json.dumps(fixture_reports,indent=2))
flamingo_reports=[]
rear_reports=[]
rear_native=(new/'policy-rear-native-0.json').exists()
rear_gated=rear_native and (new/'models/rear_hook_native_gated.onnx').exists()
rear_name='rear_hook_native_gated' if rear_gated else 'rear_hook_native' if rear_native else 'rear_hook_v27'
rear_runtime='hd1910_rear_hook_native_gated' if rear_gated else 'hd1910_rear_hook_native' if rear_native else 'hd1910_rear_hook_v27'
if rear_native:
    contract=json.loads((new/f'models/{rear_name}.metadata.json').read_text())['encoderContract']
    for n,id in enumerate(contract['servoOrder']):
        assert before['calibration']['joints']['references'][str(id)]==contract['references'][n],'Native model calibration differs'
        assert before['calibration']['joints']['directions'].get(str(id),-1)==contract['directions'][n],'Native model direction differs'
if rear_native or (new/'policy-rear-0.json').exists():
    assert not (old/f'models/{rear_name}.onnx').exists(),'Use a versioned filename for a subsequent rear-hook update'
    env=dict(os.environ,MICRODUCK_POLICY_PATH=str(new/'models/xgoduck_walk.onnx'),ORT_DYLIB_PATH=str(new/'lib/libonnxruntime.so'))
    for tick in range(4):
        fixture=new/(f'policy-rear-native-{tick}.json' if rear_native else f'policy-rear-{tick}.json')
        result=subprocess.run([str(new/'bin/microduck-observer'),'--verify-policy-fixture',str(fixture)],env=env,capture_output=True,text=True,check=True)
        rear_reports.append(json.loads(result.stdout.splitlines()[-1]));fixture.unlink()
    (stage/'rear-hook-arm-fixtures.json').write_text(json.dumps(rear_reports,indent=2))
if (new/'policy-flamingo_hold.json').exists():
    env=dict(os.environ,MICRODUCK_POLICY_PATH=str(new/'models/xgoduck_walk.onnx'),ORT_DYLIB_PATH=str(new/'lib/libonnxruntime.so'))
    for mode in ['hold','lower']:
        fixture=new/f'policy-flamingo_{mode}.json'
        result=subprocess.run([str(new/'bin/microduck-observer'),'--verify-policy-fixture',str(fixture)],env=env,capture_output=True,text=True,check=True)
        flamingo_reports.append(json.loads(result.stdout.splitlines()[-1]));fixture.unlink()
    (stage/'flamingo-arm-fixtures.json').write_text(json.dumps(flamingo_reports,indent=2))
shutil.copy2(new/'measure_runtime.py',stage/'measure_runtime.py')
shutil.copy2(new/'ws_reader.py',stage/'ws_reader.py')
model_env=dict(os.environ,MICRODUCK_POLICY_PATH=str(new/'models/xgoduck_walk.onnx'),ORT_DYLIB_PATH=str(new/'lib/libonnxruntime.so'),MICRODUCK_GAINS_FILE='/var/lib/microduck-observer/model-gains.json')
model_check=subprocess.run([str(new/'bin/microduck-observer'),'--verify-model-bank'],env=model_env,capture_output=True,text=True,check=True)
(stage/'installed-model-bank.json').write_text(model_check.stdout)
subprocess.run([sys.executable,str(new/'verify_rust_hardware.py'),'--release',str(new),'--workspace',str(stage)],check=True)
assert idle(get('snapshot'))
legacy_installation=before['calibration']['imu'].get('mountingQuaternion') is None
for tool in ['verify_rust_hardware.py','measure_runtime.py','ws_reader.py']:(new/tool).unlink()
drop=Path('/etc/systemd/system/microduck-observer.service.d/audio.conf');prior=drop.read_bytes() if drop.exists() else None
input_drop=Path('/etc/systemd/system/microduck-observer.service.d/gamepad-input.conf');input_prior=input_drop.read_bytes() if input_drop.exists() else None
policy_drop=Path('/etc/systemd/system/microduck-observer.service.d/policy-path.conf');policy_prior=policy_drop.read_bytes() if policy_drop.exists() else None
audio=b'[Service]\nSupplementaryGroups=audio\nDeviceAllow=/dev/snd/controlC1 rw\nDeviceAllow=/dev/snd/pcmC1D0p rw\n'
def point(p):
    temp=root/'current.runtime-staging';assert not temp.exists() and not temp.is_symlink();temp.symlink_to(p);os.replace(temp,root/'current')
try:
    subprocess.run(['systemctl','stop','microduck-observer'],check=True)
    if not rear_reports and not preserve_device_settings:
        subprocess.run([sys.executable,str(new/'hardware/audio/audio_prepare.py'),'--install',str(new/'hardware/audio')],check=True)
        subprocess.run(['systemctl','daemon-reload'],check=True)
        subprocess.run(['systemctl','restart','microduck-audio-prepare'],check=True)
        input_drop.write_text('[Service]\nSupplementaryGroups=input\nDeviceAllow=char-input r\n')
    point(new)
    if bank_reports:policy_drop.write_text('[Service]\nEnvironment=MICRODUCK_POLICY_PATH=/home/radxa/microduck-observer/current/models/xgoduck_walk.onnx\n')
    subprocess.run(['systemctl','daemon-reload'],check=True);subprocess.run(['systemctl','start','microduck-observer'],check=True)
    for _ in range(80):
        try:
            after=get('snapshot')
            if idle(after):break
        except Exception:pass
        time.sleep(.15)
    else:raise RuntimeError('New runtime did not become ready')
    if legacy_installation:
        defaults=json.loads((new/'imu-installation.default.json').read_text())
        expected=json.loads(json.dumps(before['calibration']))
        expected['imu'].update(defaults['imu'])
        position=expected.get('mounting',{}).get('positionMm')
        expected['mounting']=defaults['mounting']
        if position is not None:expected['mounting']['positionMm']=position
        expected['revision']+=1
        for section in ['joints','mounting','revision']:assert after['calibration'][section]==expected[section]
        for key,value in expected['imu'].items():
            if key!='validForBoot':assert after['calibration']['imu'].get(key)==value
        calpath='/var/lib/microduck-observer/calibration.json'
        preserved[calpath]=sha(Path(calpath))
    else:
        expected=before['calibration']
    assert after['calibration']['revision']==expected['revision']
    for key in ['joints','mounting']:assert after['calibration'][key]==expected[key]
    for key in ['quaternion','mountingQuaternion','targetQuaternion']:assert after['calibration']['imu'].get(key)==before['calibration']['imu'].get(key)
    assert all(sha(Path(p))==v for p,v in preserved.items())
    for p,v in preserved.items():
        p=Path(p)
        if p.is_relative_to(old) and not (p.parent==old/'models' and (p.name in retired or (walk_update and p.name==walk_update['file']))):assert sha(new/p.relative_to(old))==v
    if walk_update:
        assert sha(new/'models'/walk_update['file'])==walk_update['metadataSha256']
        policy=get('policy')
        assert policy['metadata']['walkExecution']['actionScale']==1.0
        assert get('model-gains')==json.loads((stage/'gains-before.json').read_text())
    assets={p.relative_to(new/'frontend/dist').as_posix():sha(p) for p in (new/'frontend/dist').rglob('*') if p.is_file()}
    for p,v in assets.items():assert hashlib.sha256(urllib.request.urlopen('http://127.0.0.1:8877/'+p,timeout=8).read()).hexdigest()==v
    snapshots=[get('snapshot') for _ in range(20)];assert all(idle(s) for s in snapshots)
    if fixture_reports:
        model=get('policy');assert any(m['kind']=='dance10' and m['available'] for m in model['models'])
    if flamingo_reports:
        model=get('policy');assert all(any(m['kind']==kind and m['available'] for m in model['models']) for kind in ['flamingo_hold','flamingo_lower'])
    if rear_reports:
        model=get('policy')
        assert all(any(m['kind']==kind and m['available'] and m['metadata']['runtimeKind']==rear_runtime for m in model['models']) for kind in ['flamingo_hold','flamingo_lower'])
        assert get('model-gains')==json.loads((stage/'gains-before.json').read_text())
        assert (input_drop.read_bytes() if input_drop.exists() else None)==input_prior and (drop.read_bytes() if drop.exists() else None)==prior
    if bank_reports:
        model=get('policy');expected=get('model-gains')
        assert len(model['models'])==9 and all(m['available'] and m['runtimeGains']['p']==expected['p'] and m['runtimeGains']['d']==expected['d'] for m in model['models'])
    if preserve_device_settings:
        assert input_drop.read_bytes()==input_prior and (drop.read_bytes() if drop.exists() else None)==prior
    if gamepad_mode is not None:
        request=urllib.request.Request('http://127.0.0.1:8877/api/v1/gamepad/mode',data=json.dumps({'debug':gamepad_mode}).encode(),headers={'Content-Type':'application/json'},method='POST')
        with urllib.request.urlopen(request,timeout=5) as response:assert json.load(response)['debug']==gamepad_mode
    assert {svc:subprocess.run(['systemctl','is-enabled',svc],capture_output=True,text=True).stdout.strip() for svc in ['tofd','rsyslog']}==service_states
    result={'installed':True,'release':str(new),'previousRelease':str(old),'calibrationModelsGainsIdentityUnchanged':not legacy_installation,'legacyImuInstallationMigrated':legacy_installation,'assetsHttpVerified':True,'readonlySnapshots':20,'hardwareMotion':False,'audioPermissionsConfigured':True,'inputPermissionsConfigured':True,'serviceEnableStatesPreserved':service_states,'binarySha256':sha(new/'bin/microduck-observer')}
    if walk_update:result.update(walkScaleUpdate=walk_update,calibrationModelsGainsIdentityUnchanged=False,calibrationWeightsGainsIdentityUnchanged=True)
    if flamingo_reports:result.update(flamingoModelSha256=sha(new/'models/flamingo.onnx'),flamingoArmFixtures=flamingo_reports,flamingoAvailable=True)
    if rear_reports:result.update(rearHookModelSha256=sha(new/f'models/{rear_name}.onnx'),rearHookRuntime=rear_runtime,rearHookArmFixtures=rear_reports,rearHookAvailable=True,calibrationModelsGainsIdentityUnchanged=False,calibrationGainsIdentityUnchanged=True,existingModelsUnchanged=True,physicalAccepted=False,audioSettingsUntouched=True)
    if fixture_reports:result.update(danceModelSha256=sha(new/'models/dance10.onnx'),danceArmFixtures=fixture_reports,danceAvailable=True)
    if bank_reports:result.update(retiredModels=retired,commonGainsArm=bank_reports,remainingModelsUnchanged=True)
    (stage/'deployment.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
except BaseException:
    subprocess.run(['systemctl','stop','microduck-observer'])
    if prior is None:drop.unlink(missing_ok=True)
    else:drop.write_bytes(prior)
    if input_prior is None:input_drop.unlink(missing_ok=True)
    else:input_drop.write_bytes(input_prior)
    if policy_prior is None:policy_drop.unlink(missing_ok=True)
    else:policy_drop.write_bytes(policy_prior)
    point(old);subprocess.run(['systemctl','daemon-reload']);subprocess.run(['systemctl','start','microduck-observer']);raise
'''
(stage/'install.py').write_text(installer,encoding='utf-8');compile(installer,'install.py','exec')
if args.package_only:print(stage);raise SystemExit(0)
host_key_alias=args.host_key_alias or args.host.rsplit('@',1)[-1]
opts=['-o','BatchMode=yes','-o','ConnectTimeout=8','-o','StrictHostKeyChecking=yes','-o','ServerAliveInterval=5','-o','ServerAliveCountMax=2','-o','HostKeyAlias='+host_key_alias]
if args.known_hosts:opts.extend(['-o','UserKnownHostsFile='+str(args.known_hosts.resolve())])
ssh=['ssh',*opts,'-p',str(args.port),args.host];remote='/home/radxa/runtime-stage-'+name
subprocess.run([*ssh,'mkdir -p '+remote],check=True)
subprocess.run(['scp','-O',*opts,'-P',str(args.port),str(stage/'payload.tar.gz'),str(stage/'install.py'),args.host+':'+remote+'/'],check=True)
password=next(s.removeprefix('Password: ') for s in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if s.startswith('Password: '))
result=subprocess.run([*ssh,"sudo -S -p '' python3 "+shlex.quote(remote+'/install.py')+' '+shlex.quote(remote)+' '+name],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout);print(result.stderr);result.check_returncode()
report=json.loads(result.stdout.splitlines()[-1]);(evidence/'deployment.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
