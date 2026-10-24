"""One-command fresh board deployment, with a read-only --check mode."""
import argparse
from datetime import datetime
import getpass
import hashlib
import json
from pathlib import Path
import re
import shlex
import subprocess
import time


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True, help='IP or DNS name, without user@')
    parser.add_argument('--port', type=int, default=22)
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--sha256', help='Required with an alternate --bundle')
    parser.add_argument('--password-file', type=Path, help='Local Password: file; never sent in argv')
    parser.add_argument('--check', action='store_true', help='Read-only target inspection, no install/reboot')
    args = parser.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.-]*', args.host) or not 1 <= args.port <= 65535:
        parser.error('Invalid host or port')
    known = root / 'deploy/staging/new-board-known-hosts'
    known.parent.mkdir(parents=True, exist_ok=True)
    options = ['-o', 'BatchMode=yes', '-o', 'ConnectTimeout=6',
               '-o', 'StrictHostKeyChecking=accept-new', '-o', 'UserKnownHostsFile=' + known.as_posix()]
    target = 'radxa@' + args.host
    ssh = ['ssh', *options, '-p', str(args.port), target]

    def remote(command, stdin=None, check=True):
        return subprocess.run([*ssh, command], input=stdin, text=True, encoding='utf-8',
                              capture_output=True, check=check)

    probe = remote("python3 -c " + shlex.quote(
        "import json,platform,pathlib; print(json.dumps({'architecture':platform.machine(),"
        "'hostname':platform.node(),'bootId':pathlib.Path('/proc/sys/kernel/random/boot_id').read_text().strip(),"
        "'existingPlatform':pathlib.Path('/home/radxa/microduck-observer/current').exists() or pathlib.Path('/home/radxa/microduck-observer/current').is_symlink(),"
        "'existingIdentity':pathlib.Path('/var/lib/ducklink/identity.json').exists(),"
        "'existingCalibration':pathlib.Path('/var/lib/microduck-observer/calibration.json').exists(),"
        "'board':pathlib.Path('/boot/armbianEnv.txt').read_text()}))"))
    board = json.loads(probe.stdout)
    print(json.dumps(board, ensure_ascii=False, indent=2))
    if args.check:
        return
    if board['architecture'] != 'aarch64' or any(board[k] for k in
            ['existingPlatform', 'existingIdentity', 'existingCalibration']):
        raise SystemExit('Not a fresh ARM64 board. Existing identity/calibration will not be overwritten.')
    if 'fdtfile=rockchip/rk3566-radxa-zero3.dtb' not in board['board']:
        raise SystemExit('Unsupported board')
    manifest = json.loads((root / 'deploy/bootstrap-package.json').read_text(encoding='utf-8'))
    bundle = args.bundle or (root / manifest['localPath'])
    expected = args.sha256 if args.bundle else manifest['sha256']
    if not expected or hashlib.file_digest(bundle.open('rb'), 'sha256').hexdigest() != expected:
        raise SystemExit('Bundle checksum mismatch; no installation performed')
    if args.password_file:
        password = next(line.removeprefix('Password: ') for line in
                        args.password_file.read_text(encoding='utf-8').splitlines()
                        if line.startswith('Password: '))
    else:
        password = getpass.getpass('Board sudo password (not echoed): ')
    release = 'bootstrap-' + datetime.now().strftime('%Y%m%d-%H%M%S')
    remote_dir = '/home/radxa/' + release + '-upload'
    remote('mkdir ' + shlex.quote(remote_dir))
    for path in [bundle, root / 'deploy/bootstrap_board.py', root / 'deploy/tof_prepare.py', root / 'deploy/model-gains.default.json', root / 'deploy/imu-installation.default.json']:
        subprocess.run(['scp', *options, '-P', str(args.port), str(path),
                        target + ':' + remote_dir + '/' + path.name], check=True)
    command = 'sudo -S -p \'\' python3 ' + shlex.quote(remote_dir + '/bootstrap_board.py')
    command += ' --bundle ' + shlex.quote(remote_dir + '/' + bundle.name)
    command += ' --sha256 ' + shlex.quote(expected) + ' --release ' + shlex.quote(release)
    result = remote(command, password + '\n')
    installed = json.loads(result.stdout)
    evidence = root / 'docs/实测记录/附件/平台' / datetime.now().strftime('%Y-%m-%d') / release
    evidence.mkdir(parents=True, exist_ok=False)
    (evidence / 'installed.json').write_text(json.dumps(installed, indent=2), encoding='utf-8')
    print(json.dumps(installed, ensure_ascii=False, indent=2))
    # Successful SSH disconnect during reboot is expected; verification is mandatory.
    remote("sudo -S -p '' systemctl reboot", password + '\n', check=False)
    time.sleep(15)
    deadline = time.monotonic() + 180
    last = ''
    while time.monotonic() < deadline:
        try:
            outcome = remote("python3 -c " + shlex.quote(
                "import json,subprocess,pathlib,urllib.request; "
                "h=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/health',timeout=4)); "
                "g=json.load(urllib.request.urlopen('http://127.0.0.1:8877/api/v1/model-gains',timeout=4)); "
                "saved=json.loads(pathlib.Path('/var/lib/microduck-observer/model-gains.json').read_text()); "
                "assert (g['p'],g['d'])==(saved['p'],saved['d']); "
                "boot=pathlib.Path('/proc/sys/kernel/random/boot_id').read_text().strip(); "
                "services={x:subprocess.check_output(['systemctl','is-active',x],text=True).strip() "
                "for x in ['microduck-observer','ducklink-wifi','bluetooth']}; "
                "assert h['bluetooth']['state']=='ready'; "
                "assert pathlib.Path('/dev/ttyS2').exists() and pathlib.Path('/dev/ttyS9').exists(); "
                "adv=subprocess.check_output(['busctl','get-property','org.bluez','/org/bluez/hci0',"
                "'org.bluez.LEAdvertisingManager1','ActiveInstances'],text=True).strip(); "
                "assert int(adv.split()[-1])>0; "
                "print(json.dumps({'services':services,'advertising':adv,'health':h,'bootId':boot,'modelGains':g}))"))
            verified = json.loads(outcome.stdout)
            if verified['bootId'] == board['bootId']:
                raise RuntimeError('Reboot has not completed')
            (evidence / 'verified.json').write_text(json.dumps(verified, ensure_ascii=False, indent=2), encoding='utf-8')
            print('Verified startup, UART2/9 and BLE advertising. App pairing and hardware calibration remain to be tested.')
            print('Evidence:', evidence)
            return
        except (subprocess.CalledProcessError, json.JSONDecodeError, RuntimeError) as error:
            last = getattr(error, 'stderr', None) or str(error)
            time.sleep(5)
    (evidence / 'verification-failed.txt').write_text(last, encoding='utf-8')
    raise SystemExit('Startup verification timed out. Inspect evidence/new DHCP IP; do not reinstall or rotate identity.')


if __name__ == '__main__':
    try:
        main()
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.stderr or 'SSH/transfer command failed; no success declared')
