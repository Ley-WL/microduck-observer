"""Save post-start read-only health and update the hardware record index."""
import json
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[1]
evidence = root/'docs/实测记录/附件/平台/2026-10-10/bidirectional-seed'
result = subprocess.run(['ssh','-o','BatchMode=yes','-o','ConnectTimeout=8','-o','HostKeyAlias=192.168.31.193','-p','5722','radxa@8.133.210.70','curl -fsS http://127.0.0.1:8877/api/v1/health'],check=True,capture_output=True,text=True)
health=json.loads(result.stdout)
assert health['bluetooth']['state']=='ready'
assert health['imu']['state']=='streaming'
assert not health['joints']['data']['diagnostics']['missingIds']
assert all(r['torque']==0 for r in health['joints']['data']['servos'])
(evidence/'post-start-health.json').write_text(json.dumps(health,indent=2)+'\n')
summary='> **2026-10-10 双向单actor候选已部署（最新覆盖）：** 用户选择微调前监督种子，已5722部署release20261010-bidirectional-seed-v1，仅替换alpha_sitstand权重/元数据，SHA256 2a3edcb880e88435a7ac248568859bb4f1eb487b9204db06a4aa3fb59c5ebe29。现有model-gains-v1 Rust已支持v3目标限位/坐站指令，二进制、网页、其他模型不变；旧release保留可回退。主板两方向纯推理数值校验通过、8入口available、20只读快照无缺；物理标定/15舵机关键寄存器/BLE身份/持久增益保持，重启后BLE ready、IMU streaming、全部卸力。种子CPU循环31/32是小样本仿真，非正式验收；未部署后续失败PPO或双专家组合，未发实物动作，旧坐下顶压故障仍须新模型现场验证。证据附件/平台/2026-10-10/bidirectional-seed/。\n\n'
for relative in ('AGENTS.md','docs/实测记录/README.md','docs/实测记录/03-舵机实测.md','docs/实测记录/04-网络与观测平台.md'):
    path=root/relative
    text=path.read_text(encoding='utf-8')
    if summary not in text:
        path.write_text(summary+text,encoding='utf-8')
path=root/'backend-rust/XGODUCK-SKILLS.md'
text=path.read_text(encoding='utf-8')
note='\n\n2026-10-10 当前坐站权重为用户选择的监督初始化单actor（SHA256 `2a3edcb880e88435a7ac248568859bb4f1eb487b9204db06a4aa3fb59c5ebe29`），不是原版模型或后续失败PPO。沿用hd1910_sitstand/v3合同：EMA+HOME后裁14关节目标、raw prev_action保持；坐姿持续推理，站起≥4有效秒且直立累计≥1秒后回零速walk。CPU重复循环31/32仅小样本初筛，hardwareValidated=false。release20261010-bidirectional-seed-v1已部署，实物动作待验收。\n'
if note not in text:
    path.write_text(text+note,encoding='utf-8')
print('Post-start health and records saved; no hardware motion.')
