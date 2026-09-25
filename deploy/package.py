"""Package only runtime files and the locally built frontend, never credentials."""
from pathlib import Path
import tarfile

root = Path(__file__).resolve().parents[1]
output = root / 'deploy/staging/observer.tar.gz'
output.parent.mkdir(exist_ok=True)
if not (root / 'frontend/dist/index.html').is_file():
    raise SystemExit('Build frontend first: npm run build')
with tarfile.open(output, 'w:gz') as archive:
    for name in ('server.py', 'calibrations.py', 'pose_calibration.py', 'hardware.py', 'bno085.py', 'servos.py', 'requirements.txt'):
        archive.add(root / 'debug-server' / name, arcname='debug-server/' + name)
    archive.add(root / 'frontend/dist', arcname='frontend/dist')
    for name in ('microduck-observer.service', 'install-service.sh'):
        archive.add(root / 'deploy' / name, arcname='deploy/' + name)
print(output)
