"""Record deployed trace contract and exact-input physical findings."""
from pathlib import Path

root=Path(__file__).resolve().parents[1]
summary='''> **2026-10-10 坐站逐帧原始输出最新覆盖：** 已部署release20261010-sit-trace-v1，仅更新Rust诊断与同SHA/HOME坐站历史保留，模型仍监督种子2a3edcb880…；98Rust/1忽略、9CPU回放测试及ARM构建通过，标定/15舵机关键寄存器/BLE身份/持久P/D/网页和模型保持。用户再次完成App平衡→坐→站→停止保持，保存856推理帧。坐→站历史保留已生效，但坐初期原始左/右膝输出2秒累计行程596.51/492.52°，发令354.82/314.65°，不能称抖动已修复。逐帧CPU ONNX重放误差≤2.4e-7rad、EMA误差0，支持原actor在真实反馈下振荡；双专家在同观测也振荡，单模型蒸馏/切换清历史不是已证实唯一根因。速度方向/量级无明显错误、一帧滞后相关0.983，额外延迟未改善；初态/15～35ms时延仿真未复现相同幅度。停机报告推理48Hz/反馈50Hz，不称达100Hz。证据附件/平台/2026-10-10/sit-trace/；后续离线平滑候选尚未实物验收。

'''
for relative in ('AGENTS.md','docs/实测记录/README.md','docs/实测记录/03-舵机实测.md','docs/实测记录/04-网络与观测平台.md'):
    path=root/relative
    old=path.read_text(encoding='utf-8')
    if not old.startswith(summary):path.write_text(summary+old,encoding='utf-8')
path=root/'backend-rust/XGODUCK-SKILLS.md'
addition='''
## 2026-10-10 坐站历史与逐帧诊断（已部署）

release20261010-sit-trace-v1保留同权重SHA及同HOME的sitstand_sit↔sitstand_stand原始动作/EMA；跨模型切换仍清历史和异步generation。每技能最多600推理帧、会话2000事件环形上限，保存精确obs/raw action/前后EMA/裁前裁后目标/编码及反馈；停止和故障写`/var/lib/microduck-observer/policy-session-latest.json`。故障先卸力再写文件，诊断写入失败只记warning。

用户新实测的精确ONNX重放及EMA核对通过，但原始输出已有多次反转，历史保留不是已证明的抖动修复。坐初期2秒原始膝目标累计行程596.51/492.52°，发令354.82/314.65°；实物可完成一次坐站但过程不平顺。权重仍2a3edcb880…，P/D/标定/EEPROM/保护保持。证据`docs/实测记录/附件/平台/2026-10-10/sit-trace/`。
'''
old=path.read_text(encoding='utf-8')
if addition not in old:path.write_text(old+addition,encoding='utf-8')
print('Updated four latest-coverage entries and skill execution record')
