# 官方左右脚踢球接入 — 2026-10-14

已接入独立 kick_left/kick_right ONNX，来源 pollen-robotics/microduck-policies，revision和SHA见models metadata。统一61维观测/14动作，HOME+raw、无EMA、raw历史、零13维command、P5/D20临时增益、当前机械范围及4096编码保持。为复现本轮冻结仿真，踢球不额外做电压动作缩放；不是实机PD辨识或全官方动力学一致证明。

站立行走平衡时可启动，按有效推理时间运行0.5秒后同会话返回原行走模型，摇杆需重新输入；当前UART唯一owner/反馈暂停/故障电压保护保持。执行期间不排第二个踢球；Web十字左/右，物理手柄LB+左/右（双摇杆回中、按下边沿），App动作区及更多操作提供左右踢球。BLE新增observer.kickModels，旧服务没有此能力时App禁用并提示更新，连接不会自动动作。

159 Rust测试通过/1忽略，网页72项通过及Vite构建、Android构建/lint/模拟器整套回归通过；两份native CPU纯推理fixture观测/动作最大误差均0、目标机械编码核对通过。ARM release构建通过；尚未做ARM实际推理fixture，后续部署前必须补验。编译首漏frontend测试资源及一次SCP超时导致旧source能力构建均保留；最终ducklink_motion源码SHA与服务器严格核对，再测试/ARM重构建，二进制40b0aa7e…下载SHA匹配。

DuckLink0.9.61/code80已App OTA，原APK及清单公钥保持，公网签名/APK长度SHA和下载页通过；未连接截图是Android模拟器，非实机。用户明确“暂时都不更新”187/230：未访问主板、未更新服务或模型、未发布binary-only Observer OTA（缺模型时不可只升级二进制）。integration-payload.tar.gz仅提供新binary/网页和两踢球模型，不包含现役其它模型或设备标定身份；未来须基于当时current独立release覆盖新增文件，保留全部其它模型、增益、EEPROM、BLE与masked服务并保留回滚，空闲卸力检查及ARMfixture后再安装。

小规模仿真：两脚各中心+4个±15mm球偏置，官方0.5秒切站立及3秒对照共20例均正确脚接触、球前向峰速>1m/s、未倒/末秒站姿稳定；左最终8–16cm右6–9cm。额外10cm距离门仅诊断（左每组2/5右0/5），不是官方验收或宣称远距离踢球通过。初始球无机器人穿透；基于HD1910 BAM/当前机械范围/修正DOF摩擦/供体惯量与replica接触、无齿隙、合成HOME起始/理想IMU，非完整实机动态/正式统计验收。不训练、不代发动作。
