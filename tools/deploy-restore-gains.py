"""Deploy model HOME control and frontend; preserve model, calibration and servo state."""
from pathlib import Path
import subprocess
import tarfile

root=Path(__file__).resolve().parents[1]
archive=root/'frontend/direct-pose-frontend.tar.gz'
with tarfile.open(archive,'w:gz') as tar:
    tar.add(root/'frontend/dist',arcname='dist')
opts=['-o','BatchMode=yes','-o','HostKeyAlias=192.168.31.193'];host='radxa@192.168.31.186'
for source,target in [(root/'backend-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer','observer-direct-pose.new'),
                      (root/'deploy/install-restore-gains.py','install-restore-gains.py'), (archive,'direct-pose-frontend.tar.gz')]:
    subprocess.run(['scp',*opts,str(source),f'{host}:/home/radxa/{target}'],check=True)
password=next(line.removeprefix('Password: ') for line in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if line.startswith('Password: '))
result=subprocess.run(['ssh',*opts,host,"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/install-restore-gains.py"],input=password+'\n',text=True,encoding='utf-8',capture_output=True)
print(result.stdout,result.stderr,flush=True)
directory=root/'docs/实测记录/附件/HAT/2026-10-05/restore-gains';directory.mkdir(parents=True,exist_ok=True)
subprocess.run(['scp',*opts,f'{host}:/home/radxa/direct-pose-deployment.json',str(directory/'deployment.json')],check=True)
result.check_returncode()
archive.unlink(missing_ok=True)
