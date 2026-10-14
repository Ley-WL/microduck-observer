# getup4750 模型部署（2026-10-12）

用户在查看新种子仿真后明确要求部署。主板当前 release 为 `getup4750-20261012-v2`，回滚 release 为 `frontend-20261012-094845`。

只更新 `models/xgoduck_getup.onnx` 与对应 metadata；权重 SHA256 为 `ec665f3121d8d45da0abd3d5792955d0aa46ba39f2897e3bbe01788fda9770f4`。本地 models 同步相同文件。

现有 Rust 服务已支持 `hd1910_getup`，沿用其 HOME+EMA旧0.45/新0.55、raw previous action、3秒最短执行/直立稳定1秒后回零速度行走条件。没有伪装为kissqy无EMA模型。P5/D20保留；当前kissqy机械限位裁剪保留。模型原训练仿真没有完全复现当前机械裁剪和实物固件PD，不能承诺实机同样成功。

主板 ARM 纯推理 fixture：观测误差0、动作最大误差5.96046e-8，目标机械范围检查通过，hardwareAccess=false。部署前及20次部署后快照全部15关节在线卸力。8模型入口available；二进制、网页、其余模型、全部保存标定/增益/BLE身份和舵机profiles保持，tofd/rsyslog仍masked。没有训练、自动运动、归零、写EEPROM或网络配置变化。

首v1切换后安装器将API calibration中的服务实例bootId当成保存标定，断言失败并自动回滚。随后只读比较确认唯一差异bootId；v2忽略实例bootId、仍严格校验保存文件哈希及其余全部标定值，部署通过。before-first.json/after-rollback.json保留首次证据，install.py和install-v2.log保存实际安装逻辑/最终结果。

`deployment.json`、`arm-parity.log`、`policy-after.json`及before/after保存部署证据。原release完整保留。当前实物起身效果尚未验收；用户可托稳后从App“倒地起身”入口测试，顶住立即停止。
