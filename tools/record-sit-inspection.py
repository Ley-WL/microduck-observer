"""Record observed failure separately from the unobserved sit attempt."""
import json
from pathlib import Path
root=Path(__file__).resolve().parents[1]
directory=root/'docs/实测记录/附件/平台/2026-10-10/bidirectional-sit-inspect'
failure=json.loads((directory/'failure.json').read_text(encoding='utf-8'))
timeline=json.loads((directory/'timeline.json').read_text(encoding='utf-8'))
report={
    'userReport':'App坐下似有意图但未落座，怀疑控制冲突',
    'deployedSitPolicySha256':'2a3edcb880e88435a7ac248568859bb4f1eb487b9204db06a4aa3fb59c5ebe29',
    'latestFailure':{k:failure[k] for k in ('eventTimeMs','error','stage','stats')},
    'observedControlProcessCount':1,
    'uartOwnerPid':7882,'servoUart':'/dev/ttyS2','imuUart':'/dev/ttyS9',
    'otherPythonProcess':'wifi_service.py (Wi-Fi permissions only)',
    'readonlySamples':len(timeline['samples']),
    'sitSamples':sum(r.get('control',{}).get('activeSkill')=='sitstand_sit' for r in timeline['samples']),
    'allSamplesDisabled':all(r.get('control',{}).get('state')=='disabled' for r in timeline['samples']),
    'codeChecks':['One serial owner executes one job at a time',
        'Switch drops/joins prior inference worker and clears old-generation results',
        'BLE tick and shared command route reject walking during sit skill',
        'Sit has no automatic completion transition to walk',
        'Sit command slot is twist.vx=1'],
    'conclusion':'No evidence of simultaneous model output; latest saved fault belongs to walk, not sit. Actual sit trajectory was not captured and cause remains unconfirmed.',
    'hardwareMotionSent':False,
}
(directory/'analysis.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
note='> **2026-10-10 新坐站实物反馈（最新覆盖）：** 用户反馈App坐下有动作意图但未落座，实际坐下尚未验收。只读检查一个Rust控制进程/唯一舵机UART owner；代码切换销毁旧推理线程，坐下期间BLE轮盘及共享行走入口拒绝覆盖，坐下无自动返回walk。最新保存故障却为activeSkill=xgoduck、供体walk哈希5727b909…，#12电压3.8V卸力，故障前多关节负载±1000；不能冒称这是新坐下模型的故障过程。50秒500次只读采样全部disabled、零坐下帧，尚不能判定坐下失败根因；未发动作、未改增益/保护/标定/服务。需后续捕获sitstand_sit目标与反馈过程，证据附件/平台/2026-10-10/bidirectional-sit-inspect/。\n\n'
for relative in ('AGENTS.md','docs/实测记录/README.md','docs/实测记录/03-舵机实测.md','docs/实测记录/04-网络与观测平台.md'):
    path=root/relative
    text=path.read_text(encoding='utf-8')
    if note not in text:path.write_text(note+text,encoding='utf-8')
print('Inspection evidence saved; actual sit cause unconfirmed.')
