"""Read-only evidence for the independent mouth control during native pick."""
from pathlib import Path
import json, subprocess, hashlib

root = Path(__file__).resolve().parents[1]
out = root / 'docs/实测记录/附件/平台/2026-10-11/pick-mouth'
out.mkdir(parents=True, exist_ok=True)
ssh = ['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=8', '-o', 'ServerAliveInterval=5', '-o', 'ServerAliveCountMax=2', '-o', 'HostKeyAlias=192.168.31.193', '-p', '5722', 'radxa@8.133.210.70']
def read(command):
    return subprocess.run(ssh + [command], capture_output=True, check=True, timeout=25).stdout
snapshot = json.loads(read('curl -fsS --max-time 4 http://127.0.0.1:8877/api/v1/snapshot'))
(out / 'snapshot.json').write_text(json.dumps(snapshot, ensure_ascii=False), encoding='utf-8')
audit = {'hardwareMotion': False, 'codeChanged': False, 'training': False,
         'cause': 'native pick has 14 actor outputs excluding mouth; session initializes independent mouth intent to -5 degrees; no pick auto-open script',
         'remainingMismatch': 'local queue_mouth and UI reject mouth while pick runs; donor robotd independent mouth intent is not limited to walking',
         'control': snapshot['system']['data']['servoControl'],
         'release': snapshot['system']['data']['resources']['board']['release']}
try:
    trace_bytes = read('cat /var/lib/microduck-observer/policy-session-latest.json')
    trace = json.loads(trace_bytes)
    (out / 'session-latest.json').write_bytes(trace_bytes)
    frames = [f for f in trace.get('frames', []) if f.get('type') == 'inference' and f.get('kind') == 'xgoduck_pick']
    audit['latestPickFrames'] = len(frames)
    audit['pickMouthGoalsRaw'] = sorted({f.get('goals', {}).get('34') for f in frames if f.get('goals', {}).get('34') is not None})
except (subprocess.SubprocessError, ValueError) as error:
    audit['traceReadError'] = str(error)
audit['sourceHashes'] = {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in ['backend-rust/src/policy.rs', 'backend-rust/src/telemetry.rs', 'frontend/src/components/ModelRuntime.vue']}
(out / 'audit.json').write_text(json.dumps(audit, ensure_ascii=False, indent=2), encoding='utf-8')
note = '> **2026-10-11 拾取不张嘴核查（只读）：** 用户反馈拾取下蹲时嘴不张。当前native-v2拾取模型14维输出不包含#34嘴；上次kissqy对齐移除了旧自动开合脚本，会话嘴意图初始化−5°，native循环只消费独立pending_mouth，所以没有自动张嘴目标。另确认本机queue_mouth要求activeSkill=xgoduck，网页嘴按钮也受行走/空闲门控，拾取期间无法独立张嘴；供体robotd嘴意图按ready/非跌倒/传感器有效条件执行，没有仅行走限制，是仍未对齐的接口差异。主板只读快照及最后会话保存附件/平台/2026-10-11/pick-mouth/；最后会话若无pick帧不能冒称实测嘴故障。没有发动作、训练、部署或改标定增益。\n\n'
for name in ['AGENTS.md', 'docs/实测记录/03-舵机实测.md', 'docs/实测记录/04-网络与观测平台.md', 'docs/实测记录/README.md']:
    path = root / name
    text = path.read_text(encoding='utf-8')
    if '2026-10-11 拾取不张嘴核查（只读）' not in text:
        path.write_text(note + text, encoding='utf-8')
print(json.dumps(audit, ensure_ascii=False))
