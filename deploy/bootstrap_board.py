"""Fresh Radxa ZERO 3W installer. Never update an already provisioned board."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import tarfile
import uuid


def run(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def seed_model_gains(state, defaults):
    value = json.loads(defaults.read_text())
    if set(value) != {'p', 'd'} or any(type(v) is not int or not 0 <= v <= 255 for v in value.values()):
        raise ValueError('Invalid default model gains')
    target = state / 'model-gains.json'
    try:
        with target.open('x') as output:
            json.dump(value, output)
    except FileExistsError:
        pass  # Never replace a saved platform setting.
    return target


def seed_imu_installation(state):
    defaults = json.loads(Path(__file__).with_name('imu-installation.default.json').read_text())
    value = {'schema': 1, 'deviceId': str(uuid.uuid4()), 'revision': 0, 'updatedAt': 0,
             'joints': {'initialized': False, 'references': {}, 'directions': {}},
             'mounting': defaults['mounting'],
             'imu': {**defaults['imu'], 'initialized': False, 'quaternion': None, 'bootId': '', 'time': ''}}
    target = state / 'calibration.json'
    try:
        with target.open('x') as output:
            json.dump(value, output)
        target.chmod(0o600)
    except FileExistsError:
        pass
    return target


def boot_configuration(text):
    values = dict(line.split('=', 1) for line in text.splitlines() if '=' in line and not line.startswith('#'))
    if values.get('fdtfile') != 'rockchip/rk3566-radxa-zero3.dtb':
        raise RuntimeError('Unsupported board DTB; no configuration written')
    overlays = list(dict.fromkeys(values.get('overlays', '').split() +
                                 ['uart2-m0', 'uart9-m1', 'i2c3-pihat', 'aic3104-i2c3']))
    preserved = [line for line in text.splitlines()
                 if not line.startswith(('console=', 'overlay_prefix=', 'overlays='))]
    return '\n'.join(preserved + ['console=display', 'overlay_prefix=rk3568',
                                  'overlays=' + ' '.join(overlays)]) + '\n'


def require_fresh(root):
    for name in ['home/radxa/microduck-observer/current', 'var/lib/ducklink/identity.json',
                 'var/lib/microduck-observer/calibration.json']:
        p = root / name
        if p.exists() or p.is_symlink():
            raise RuntimeError('Existing device state: ' + name + '; use the runtime updater')


def unpack(bundle, destination):
    allowed = ['release/', 'system/etc/systemd/system/', 'system/etc/dbus-1/system.d/',
               'system/usr/local/lib/microduck-services/',
               'system/usr/local/lib/microduck-observer-ota-helper/']
    with tarfile.open(bundle) as archive:
        for member in archive.getmembers():
            name = member.name.rstrip('/')
            if member.isdev() or member.issym() or member.islnk():
                raise RuntimeError('Bundle must contain regular files/directories: ' + name)
            if not any((name + '/').startswith(prefix) for prefix in allowed):
                raise RuntimeError('Unexpected bundle path: ' + name)
            if '..' in Path(name).parts:
                raise RuntimeError('Unsafe bundle path')
        archive.extractall(destination, filter='data')


def install(bundle, digest, release_name):
    if os.geteuid() != 0 or run('uname', '-m').strip() != 'aarch64':
        raise RuntimeError('Requires root on ARM64')
    require_fresh(Path('/'))
    defaults = Path(__file__).with_name('model-gains.default.json')
    if json.loads(defaults.read_text()) != {'p': 5, 'd': 0}:
        raise RuntimeError('New-board defaults must be P5/D0')
    if hashlib.file_digest(Path(bundle).open('rb'), 'sha256').hexdigest() != digest:
        raise RuntimeError('Transferred bundle checksum mismatch')
    env = Path('/boot/armbianEnv.txt')
    original = env.read_text()
    updated = boot_configuration(original)
    for name in ['uart2-m0', 'uart9-m1', 'i2c3-pihat', 'aic3104-i2c3']:
        if not Path('/boot/dtb/rockchip/overlay/rk3568-' + name + '.dtbo').is_file():
            raise RuntimeError('Missing overlay: ' + name)
    work = Path('/home/radxa') / release_name
    work.mkdir(exist_ok=False)
    unpack(bundle, work)
    binary = work / 'release/bin/microduck-observer'
    header = binary.read_bytes()[:20]
    if header[:4] != b'\x7fELF' or int.from_bytes(header[18:20], 'little') != 183:
        raise RuntimeError('Not an ARM64 service')
    if 'not found' in run('ldd', str(binary)):
        raise RuntimeError('Missing runtime libraries; nothing installed')
    # Marker and staged files are retained for diagnosis if installation fails.
    marker = Path('/var/lib/microduck-bootstrap') / release_name
    marker.mkdir(parents=True, exist_ok=False)
    (marker / 'armbianEnv.before').write_text(original)
    (marker / 'hostname.before').write_text(Path('/etc/hostname').read_text())
    for group in ['robot', 'ducklink', 'i2c']:
        run('groupadd', '-f', group)
    if subprocess.run(['id', 'ducklink'], capture_output=True).returncode:
        run('useradd', '--system', '--gid', 'ducklink', '--no-create-home',
            '--shell', '/usr/sbin/nologin', 'ducklink')
    run('usermod', '-aG', 'dialout,bluetooth,ducklink,robot,audio,i2c', 'radxa')
    root = Path('/home/radxa/microduck-observer')
    release = root / 'releases' / release_name
    release.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(work / 'release', release)
    for source in (work / 'system').rglob('*'):
        if source.is_file():
            target = Path('/') / source.relative_to(work / 'system')
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
    (root / 'current').symlink_to(release)
    run('chown', '-R', 'radxa:radxa', str(root))
    robot = secrets.token_hex(8)
    hostname = 'microduck-' + robot[10:]
    identity = Path('/var/lib/ducklink/identity.json')
    identity.parent.mkdir(exist_ok=True)
    identity.write_text(json.dumps({'robotId': robot,
                                   'setupCode': '00000000000000000000000000123456', 'owner': None}))
    os.chmod(identity, 0o600)
    run('chown', '-R', 'radxa:ducklink', str(identity.parent))
    state = Path('/var/lib/microduck-observer')
    state.mkdir(exist_ok=True)
    gains_file = seed_model_gains(state, defaults)
    calibration_file = seed_imu_installation(state)
    run('chown', 'radxa:radxa', str(calibration_file))
    run('chown', 'radxa:radxa', str(gains_file))
    run('chown', 'radxa:radxa', str(state))
    run('hostnamectl', 'set-hostname', hostname)
    env.write_text(updated)
    run('systemctl', 'daemon-reload')
    run('systemctl', 'enable', 'microduck-observer', 'ducklink-wifi',
        'microduck-power.socket', 'bluetooth')
    run('systemctl', 'reload', 'dbus')
    run('sync')
    result = {'hostname': hostname, 'bluetoothName': 'DuckLink-' + robot[10:],
              'robotId': robot, 'release': str(release), 'rebootRequired': True,
              'binarySha256': hashlib.sha256((release / 'bin/microduck-observer').read_bytes()).hexdigest(),
              'motionSent': False}
    result['modelGains'] = json.loads(gains_file.read_text())
    (marker / 'installed.json').write_text(json.dumps(result, indent=2))
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle', required=True)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--release', required=True)
    args = parser.parse_args()
    if not args.release.replace('-', '').isalnum() or len(args.release) > 64:
        parser.error('Invalid release name')
    print(json.dumps(install(args.bundle, args.sha256, args.release)))
