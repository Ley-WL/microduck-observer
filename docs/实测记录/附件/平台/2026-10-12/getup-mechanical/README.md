# 新机械限位起身部署

2026-10-12 新限位起身模型已按用户要求提前部署（覆盖此前未部署状态）：release getup-mechanical-20261012-v2，模型SHA4b9c82e5bbd003a465b9bf861d4e7020014e7781d311f3780001e5257a588838。用户明确“先部署，再看验证进度”，正式仿真未结束，不称已通过正式/实机验收。仅替换起身ONNX/metadata和Rust二进制f41d2271…（增加新机械训练合同严格校验：14目标/物理范围、任务、执行revision；控制算法不改）。现行kissqy机械限位/EMA旧.45新.55/raw历史/最少3有效秒+直立稳定1秒/P5D20保持；其它模型/网页/helper/标定EEPROM/IMU/BLE身份/持久增益/15舵机profiles逐项保持，tofd/rsyslog仍masked。127Rust检查通过/1外部忽略，ARM构建通过；板纯推理观测误差0/输出2.980e-8rad，机械目标校验/8入口available/20只读卸力快照通过，未代发动作。首v1在切换前因上传二进制没有执行位拒绝，生产服务未变，v2显式chmod755后完成，旧getup4750-v2保留回滚。完整部署证据已下载哈希核验，附件/平台/2026-10-12/getup-mechanical/deployment.json及download-audit.json。

正式仿真launcher718123仍在运行、无新训练或GPU；2026-10-12T05:55:43.328657+00:00快照正常1810/2400、扰动1859/2400，合3669/4800（76.4%）。两条件各完成seed517031六姿和seed629017前三姿（9/12组）；已完成每组≥95%，seed517031倒置正常199/200、扰动198/200，其余已完成组200/200。当前正在seed629017右侧倒地，尚余斜倒/倒置；不把未完成部分或本轮仿真称实机已成功。进度附件getup-formal-progress.json；不要重复启动718123。

部署纯推理与只读验证记录：deployment.json、arm-parity.log、before.json、after.json、policy-after.json、download-audit.json。

未执行运动；实机效果需用户托稳验证。
