"""Deploy only the selected sitstand actor using the reviewed rollback installer."""
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[1]
stage = root / 'deploy/staging/bidirectional-seed-20261010'
stage.mkdir(parents=True, exist_ok=True)
script = (root / 'deploy/install-hd1910-skills.py').read_text()
script = script.replace('Install qualified replica/HD1910 recovery and sitstand models', 'Install user-selected supervised sitstand candidate')
script = script.replace("'20261010-hd1910-skills-v1'", "'20261010-bidirectional-seed-v1'")
script = script.replace('/home/radxa/hd1910-skills-stage', '/home/radxa/bidirectional-seed-stage')
script = script.replace("shutil.copy2('/home/radxa/bidirectional-seed-stage/observer.new', release/'bin/microduck-observer')", "assert sha(release/'bin/microduck-observer') == 'c81aeffe7cd396be7fa30f93f1782b756cab731366095c18d67b2ecaf2de4f8a', 'Unreviewed service binary'")
script = script.replace("('xgoduck_getup','alpha_sitstand')", "('alpha_sitstand',)")
script = script.replace("    assert all(value['worstGroupSuccessRate']>=.95 for value in metadata['simulationAcceptance'].values())", "    assert metadata['policySha256']=='2a3edcb880e88435a7ac248568859bb4f1eb487b9204db06a4aa3fb59c5ebe29'\n    assert metadata['skillPhysicsRevision']=='sit-self-contact-bounded-target-v3'\n    assert metadata['deploymentProvenance']['formalAcceptancePassed'] is False\n    assert metadata['hardwareValidated'] is False")
start = script.index('# Extract reviewed frontend')
end = script.index('simulation_cal =', start)
script = script[:start] + "manifest['frontendAfter']=tree(release/'frontend')\n" + script[end:]
script = script.replace("('policy-xgoduck_getup.json','policy-sitstand-sit.json','policy-sitstand-stand.json')", "('policy-sitstand-sit.json','policy-sitstand-stand.json')")
script = script.replace("[('xgoduck_getup','xgoduck_getup'),('sitstand_sit','alpha_sitstand'),('sitstand_stand','alpha_sitstand')]", "[('sitstand_sit','alpha_sitstand'),('sitstand_stand','alpha_sitstand')]")
script = script.replace('before = ready()', "gains=cal.with_name('model-gains.json')\ngains_before=gains.read_bytes() if gains.exists() else None\nbefore = ready()")
script = script.replace("    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'", "    assert sha(identity)==manifest['identityBefore'], 'BLE identity changed'\n    assert (gains.read_bytes() if gains.exists() else None)==gains_before, 'Model gains changed'\n    manifest['modelGainsUnchanged']=True")
installer = stage / 'install.py'
installer.write_text(script)
compile(script, str(installer), 'exec')
host = 'radxa@8.133.210.70'
ssh = ['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=8', '-o', 'HostKeyAlias=192.168.31.193', '-p', '5722', host]
scp = ['scp', '-O', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=8', '-o', 'HostKeyAlias=192.168.31.193', '-P', '5722']
remote = '/home/radxa/bidirectional-seed-stage/'
subprocess.run([*ssh, 'mkdir -p ' + remote], check=True)
files = [installer, root/'models/alpha_sitstand.onnx', root/'models/alpha_sitstand.metadata.json']
files += [root/f'backend-rust/tests/fixtures/policy-sitstand-{name}.json' for name in ('sit', 'stand')]
for file in files:
    subprocess.run([*scp, str(file), host + ':' + remote + file.name], check=True)
credentials = Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt')
password = next(line.removeprefix('Password: ') for line in credentials.read_text().splitlines() if line.startswith('Password: '))
result = subprocess.run([*ssh, "sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python " + remote + 'install.py'], input=password+'\n', text=True, encoding='utf-8', capture_output=True)
print(result.stdout, result.stderr, flush=True)
evidence = root/'docs/实测记录/附件/平台/2026-10-10/bidirectional-seed'
evidence.mkdir(parents=True, exist_ok=True)
if result.returncode == 0:
    subprocess.run([*scp, host+':/home/radxa/20261010-bidirectional-seed-v1-deployment.json', str(evidence/'deployment.json')], check=True)
result.check_returncode()
