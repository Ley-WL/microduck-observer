"""Upload native binary/frontend and execute the guarded read-only installer."""
from pathlib import Path
import subprocess
import tarfile

root = Path(__file__).resolve().parents[1]
opts = ['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193']
host = 'radxa@192.168.31.186'
staging = root/'deploy/staging/observer-angle-frontend.tar.gz'
staging.parent.mkdir(parents=True,exist_ok=True)
dist = root/'frontend/dist'
with tarfile.open(staging,'w:gz') as archive:
    for path in dist.iterdir():
        archive.add(path,arcname=path.name)
for path,remote in [(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer','observer-servo-angle-feedback.new'),
                    (staging,'observer-angle-frontend.tar.gz'),
                    (root/'deploy/install-servo-angle-feedback.py','install-servo-angle-feedback.py')]:
    subprocess.run(['scp',*opts,str(path),f'{host}:/home/radxa/{remote}'],check=True)
password = next(line.removeprefix('Password: ') for line in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if line.startswith('Password: '))
result = subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/install-servo-angle-feedback.py"],
                        input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
evidence = root/'docs/实测记录/附件/HAT/2026-10-04/servo-angle-feedback'
evidence.mkdir(parents=True,exist_ok=True)
subprocess.run(['scp',*opts,f'{host}:/home/radxa/servo-angle-feedback-deployment.json',str(evidence/'deployment.json')],check=True)
result.check_returncode()
