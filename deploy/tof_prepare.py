"""Prepare only J5 I2C3 permissions; never sends sensor or servo commands."""
import argparse
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import time

BUS = Path('/dev/i2c-3')
STATUS = Path('/run/microduck-tof-permissions.json')
UNIT = '''[Unit]
Description=Prepare MicroDuck J5 ToF permissions
After=systemd-modules-load.service systemd-udev-trigger.service
Before=microduck-observer.service
[Service]
Type=oneshot
ExecStart=/usr/bin/python3 /usr/local/lib/microduck-services/tof_prepare.py
TimeoutStartSec=8
NoNewPrivileges=true
'''
DROP = '''[Unit]
Wants=microduck-tof-prepare.service
After=microduck-tof-prepare.service
[Service]
Environment=MICRODUCK_TOF_BUS=/dev/i2c-3
SupplementaryGroups=i2c
DeviceAllow=/dev/i2c-3 rw
'''
RULE = 'SUBSYSTEM=="i2c-dev", KERNEL=="i2c-3", GROUP="i2c", MODE="0660"\n'


def fix_permissions(path=BUS):
    import grp
    fd = os.open(path, os.O_RDONLY | os.O_CLOEXEC | os.O_NONBLOCK | os.O_NOFOLLOW)
    try:
        info = os.fstat(fd)
        if not stat.S_ISCHR(info.st_mode) or (os.major(info.st_rdev), os.minor(info.st_rdev)) != (89, 3):
            raise RuntimeError('Expected the I2C3 character device (89:3)')
        os.fchown(fd, 0, grp.getgrnam('i2c').gr_gid)
        os.fchmod(fd, 0o660)
    finally:
        os.close(fd)


def prepare(status_path=STATUS, wait_seconds=2):
    result = {'ready': False, 'bus': str(BUS), 'motionSent': False, 'sensorCommandsSent': False,
              'preparedAtUnixMs': int(time.time() * 1000), 'pid': os.getpid()}
    try:
        if not BUS.exists():
            subprocess.run(['modprobe', 'i2c-dev'], timeout=3, capture_output=True, check=False)
        deadline = time.monotonic() + wait_seconds
        while not BUS.exists() and time.monotonic() < deadline:
            time.sleep(.1)
        fix_permissions()
        result.update(ready=True, mode='0660', group='i2c')
    except Exception as error:
        result['error'] = str(error)
    # Missing hardware must not prevent the platform from starting.
    with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', dir=status_path.parent,
                                     prefix='.microduck-tof-', delete=False) as output:
        json.dump(result, output)
        temporary = Path(output.name)
    temporary.replace(status_path)
    return result


def install():
    subprocess.run(['groupadd', '-f', 'i2c'], check=True)
    helper = Path('/usr/local/lib/microduck-services/tof_prepare.py')
    helper.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(__file__, helper)
    helper.chmod(0o644)
    files = {
        Path('/etc/systemd/system/microduck-tof-prepare.service'): UNIT,
        Path('/etc/systemd/system/microduck-observer.service.d/tof-j5.conf'): DROP,
        Path('/etc/udev/rules.d/99-microduck-tof-j5.rules'): RULE,
    }
    for path, content in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding='utf-8')
        path.chmod(0o644)
    subprocess.run(['udevadm', 'control', '--reload-rules'], check=True)
    subprocess.run(['udevadm', 'trigger', '--action=change', '--subsystem-match=i2c-dev',
                    '--sysname-match=i2c-3'], check=True)
    subprocess.run(['udevadm', 'settle', '--timeout=5'], check=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install', action='store_true')
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise SystemExit('Requires root; use the installed systemd preparation service')
    if args.install:
        install()
    else:
        print(json.dumps(prepare()))
