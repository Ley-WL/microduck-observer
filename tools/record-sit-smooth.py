"""Preserve short supervised repair and deployment scope in physical records."""
from pathlib import Path
root=Path(__file__).resolve().parents[1]
summary='''> **2026-10-10 坐站平滑候选已部署（最新覆盖）：** release20261010-sit-smooth-v1，alpha_sitstand SHA85d7e1a48f…；基于实机原始输出振荡记录做300次监督梯度更新，以完整仿真轨迹保留约束+真实同指令相邻输出平滑损失修正MLP，保留原ONNX归一化图/61→14/EMA0.45/原始prev_action/限位/接触合同；不是新1000轮PPO或手工插值。五档候选中选择weight1；更强3/10在独立96例分别86/85通过，原88，已排除。选中候选与原配对16/96/32共144例，两者均134通过，所有原成功例无新增失败；仍小样本accepted=false，未称完全可靠。相同真实观测回放（拟合内样本、非新实机闭环）坐初2秒发令两膝累计行程354.82/314.76→233.77/212.25°，降低34.1%/32.6%，逐帧14维输出变化RMS降低46.8%。主板两方向ARM数值核对/8入口available/20快照零缺失，服务二进制/网页/其他模型/物理标定/15寄存器/BLE身份/持久增益保持，未发动作；上一release sit-trace-v1保留可回退。新权重实际抖动改善尚待用户再做一次坐站，不能拿旧试验冒充新模型实物验收。证据附件/平台/2026-10-10/sit-smooth/deployment.json及平级RL artifacts/sit-smooth-repair-20261010/。

'''
for relative in ('AGENTS.md','docs/实测记录/README.md','docs/实测记录/03-舵机实测.md','docs/实测记录/04-网络与观测平台.md'):
    path=root/relative;old=path.read_text(encoding='utf-8')
    if not old.startswith(summary):path.write_text(summary+old,encoding='utf-8')
path=root/'backend-rust/XGODUCK-SKILLS.md'
addition='''
## 2026-10-10 平滑actor候选（已部署，实物改善待验证）

当前alpha_sitstand是短程监督修正的单actor，SHA85d7e1a48f97d29af2482ad92ecf0685837e520d01960f34642feed0c82fc167；release20261010-sit-smooth-v1。只修改MLP权重，原图归一化/运行端EMA/HOME/目标裁剪/动作历史保持。以同指令新鲜相邻实机输入的输出差做平滑成本，完整仿真轨迹及全部实机输出做行为保留锚；300步，非PPO续训。新旧144例配对均134成功，无新增失败，但未正式验收。真实观测重放是拟合内结果，不能代表实物新闭环改善。

部署保留Rust trace二进制、所有其他模型、网页、标定、关键舵机寄存器、BLE身份和持久增益。旧sit-trace-v1完整保留回退。部署脚本tools/deploy-sit-smooth.py先检查三组配对无新增失败，再复用活动任务拒绝、ARM数值核对、寄存器/标定核对及失败回退流程；不自动运动。硬件验收仍待用户新一次App坐站。
'''
old=path.read_text(encoding='utf-8')
if addition not in old:path.write_text(old+addition,encoding='utf-8')
print('Recorded model candidate deployment and validation limits')
