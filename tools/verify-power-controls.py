"""Read-only deployment checks; never sends an accepted power request."""
import json
from pathlib import Path
import re
import subprocess
import urllib.error
import urllib.request

base = 'http://192.168.31.186:8877'
info = json.load(urllib.request.urlopen(base + '/api/v1/info'))
results = []
for action, body, origin in [
    ('reboot', {}, None),
    ('reboot', {'confirm': True, 'bootId': 'stale'}, None),
    ('invalid', {'confirm': True, 'bootId': info['bootId']}, None),
    ('reboot', {'confirm': True, 'bootId': info['bootId']}, 'http://evil.invalid'),
]:
    headers = {'Content-Type': 'application/json'}
    if origin:
        headers['Origin'] = origin
    req = urllib.request.Request(base + '/api/v1/system/' + action, data=json.dumps(body).encode(), headers=headers)
    try:
        urllib.request.urlopen(req)
        raise RuntimeError('Unexpected acceptance')
    except urllib.error.HTTPError as error:
        assert error.code == (403 if origin else 409)
        results.append({'case': action, 'status': error.code})
root = Path(__file__).resolve().parents[1]
page = urllib.request.urlopen(base + '/').read()
assert page == (root / 'frontend/dist/index.html').read_bytes()
for asset in re.findall(r'(?:src|href)="(/assets/[^"]+)"', page.decode()):
    assert urllib.request.urlopen(base + asset).read() == (root / 'frontend/dist' / asset.lstrip('/')).read_bytes()
code = '''import socket
s=socket.socket(socket.AF_UNIX)
s.connect('/run/microduck-power.sock')
s.sendall(b'{"action":"invalid"}\\n')
s.shutdown(socket.SHUT_WR)
print(s.recv(4096).decode())
'''
result = subprocess.run(['ssh', '-o', 'BatchMode=yes', '-o', 'HostKeyAlias=192.168.31.193',
                         'radxa@192.168.31.186', 'python3 -'], input=code, text=True, capture_output=True, check=True)
assert json.loads(result.stdout)['accepted'] is False
report = {'rejections': results, 'frontendAssetsMatch': True, 'restrictedSocketResponds': True, 'actualPowerActionTested': False}
dest = root / 'docs/实测记录/附件/平台/2026-10-03/power-controls-verification.json'
dest.write_text(json.dumps(report), encoding='utf-8')
print(json.dumps(report))
