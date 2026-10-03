"""Upload isolated release and invoke the board installer with existing credentials."""
from pathlib import Path
import subprocess
import tarfile

root = Path(__file__).resolve().parents[1]
bundle = root / 'deploy/staging/power-controls.tar.gz'
with tarfile.open(bundle, 'w:gz') as archive:
    archive.add(root / 'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer', arcname='microduck-observer')
    archive.add(root / 'frontend/dist', arcname='dist')
    for name in ('power-helper.py', 'microduck-power.socket', 'microduck-power@.service', 'install-power-controls.py'):
        archive.add(root / 'deploy' / name, arcname=name)
host = 'radxa@192.168.31.186'
options = ['-o', 'BatchMode=yes', '-o', 'HostKeyAlias=192.168.31.193', '-o', 'ConnectTimeout=8']
subprocess.run(['scp', *options, str(bundle), host + ':/home/radxa/power-controls.tar.gz'], check=True)
subprocess.run(['ssh', *options, host, 'mkdir -p /home/radxa/power-controls-stage && tar -xzf /home/radxa/power-controls.tar.gz -C /home/radxa/power-controls-stage'], check=True)
login = Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text()
password = next(line.removeprefix('Password: ') for line in login.splitlines() if line.startswith('Password: '))
result = subprocess.run(['ssh', *options, host, "sudo -S -p '' python3 /home/radxa/power-controls-stage/install-power-controls.py"],
                        input=password + '\n', text=True, encoding='utf-8', capture_output=True)
print(result.stdout, result.stderr)
raise SystemExit(result.returncode)
