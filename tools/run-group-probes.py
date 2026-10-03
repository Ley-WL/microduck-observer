"""Run specified groups; archive each raw trial before proceeding."""
from pathlib import Path
import json, subprocess, sys, time

root = Path(__file__).resolve().parent
dest = root.parent / 'docs/实测记录/附件/HAT/2026-10-03'
opts = ['-o', 'BatchMode=yes', '-o', 'HostKeyAlias=192.168.31.193']
host = 'radxa@192.168.31.186'
password = next(x.removeprefix('Password: ') for x in Path('D:/Users/wl/Downloads/MicroDuck-Flash/board-login-20260928.txt').read_text().splitlines() if x.startswith('Password: '))
groups = sys.argv[1:]
summary = []
for group in groups:
    script = f'test-group-{group}-board.py'
    subprocess.run(['scp', *opts, str(root/script), f'{host}:/home/radxa/{script}'], check=True)
for n in range(1, 4):
    for group in groups:
        result = subprocess.run(['ssh', *opts, host, f"sudo -S -p '' /home/radxa/microduck-observer/venv/bin/python /home/radxa/test-group-{group}-board.py"], input=password+'\n', text=True, encoding='utf-8', capture_output=True)
        print(group, n, result.stdout, result.stderr, flush=True)
        result.check_returncode()
        target = dest / f'group-probe-{group}-{n}.json'
        subprocess.run(['scp', *opts, f'{host}:/home/radxa/group-probe-{group}-20261003.json', str(target)], check=True)
        data = json.loads(target.read_text())
        summary.append({'group': group, 'round': n, 'result': data['result'], 'error': data.get('error'), 'timing': data['timing']})
        (dest/'group-probe-summary.json').write_text(json.dumps(summary), encoding='utf-8')
        if data.get('unload', {}).get('unconfirmedIds') or data.get('unload', {}).get('state') != 'disabled':
            raise SystemExit('卸力未确认，停止后续测试')
        if 'baseline' not in data or 'enabled' not in data:
            raise SystemExit('预检失败，停止后续测试')
        time.sleep(1)
