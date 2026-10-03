"""One bounded request over a systemd-owned Unix socket; no shell execution."""
import json
import subprocess
import sys


def handle(data, run=subprocess.run):
    if not isinstance(data, dict) or set(data) != {'action'} or data['action'] not in ('poweroff', 'reboot'):
        return {'accepted': False, 'detail': 'Invalid power action'}
    result = run([
        '/usr/bin/systemd-run', '--quiet', '--unit=microduck-power-pending',
        '--on-active=5s', '--timer-property=AccuracySec=1s',
        '/usr/bin/systemctl', data['action'],
    ], capture_output=True, text=True, timeout=2)
    if result.returncode:
        return {'accepted': False, 'detail': '已有电源请求或系统调度失败'}
    return {'accepted': True, 'action': data['action'], 'delaySeconds': 5}


if __name__ == '__main__':
    try:
        reply = handle(json.loads(sys.stdin.buffer.readline(257)))
    except Exception:
        reply = {'accepted': False, 'detail': '电源请求失败'}
    print(json.dumps(reply), flush=True)
