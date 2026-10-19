"""Capture external UART loopback evidence without modifying device calibration."""
import argparse
from datetime import datetime
import getpass
import json
from pathlib import Path
import re
import shlex
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True)
    parser.add_argument('--password-file', type=Path)
    parser.add_argument('--confirmed-isolated', action='store_true', required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.-]*', args.host):
        parser.error('Invalid host')
    root = Path(__file__).resolve().parents[1]
    known = root / 'deploy/staging/new-board-known-hosts'
    known.parent.mkdir(parents=True, exist_ok=True)
    options = ['-o', 'BatchMode=yes', '-o', 'ConnectTimeout=6',
               '-o', 'StrictHostKeyChecking=accept-new', '-o', 'UserKnownHostsFile=' + known.as_posix()]
    host = 'radxa@' + args.host
    stamp = datetime.now().strftime('%Y%m%d-%H%M%S')
    remote = '/home/radxa/check-uart-loopback-' + stamp + '.py'
    subprocess.run(['scp', *options, str(root / 'deploy/check_uart_loopback.py'), host + ':' + remote], check=True)
    password = (next(line.removeprefix('Password: ') for line in
                     args.password_file.read_text(encoding='utf-8').splitlines() if line.startswith('Password: '))
                if args.password_file else getpass.getpass('Board sudo password: '))
    result = subprocess.run(['ssh', *options, host,
                             "sudo -S -p '' python3 " + shlex.quote(remote) + ' --confirmed-isolated'],
                            input=password + '\n', text=True, encoding='utf-8', capture_output=True)
    evidence = root / 'docs/实测记录/附件/HAT' / datetime.now().strftime('%Y-%m-%d') / ('loopback-' + stamp)
    evidence.mkdir(parents=True, exist_ok=False)
    (evidence / 'result.json').write_text(result.stdout or '{}', encoding='utf-8')
    (evidence / 'stderr.txt').write_text(result.stderr, encoding='utf-8')
    if result.returncode not in [0, 2]:
        raise SystemExit('Test could not run. Evidence: ' + str(evidence))
    report = json.loads(result.stdout)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    print('Evidence:', evidence)
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
